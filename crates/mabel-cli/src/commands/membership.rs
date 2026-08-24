//! `mabel membership invite|accept|admit|remove|list`.
//!
//! Three commands and three files carry one admission (proposal 002 section
//! 6). A controller appends the invitation and hands out an
//! `InvitationBundle`; the invitee reads it, sees what accepting means, signs
//! an `AcceptanceFile`; a controller appends that. Nobody is added to a ledger
//! without their own signature (decision 004).
//!
//! Every append goes through [`crate::append::append`], so the fold decides
//! what lands: an invitation of an identity that already has an open one, a
//! removal of the raw root and a removal that would leave no controller are
//! refused there, with the fold's own reason and exit 20.
//!
//! One failure is spelled here rather than left to the fold: replaying an
//! acceptance the ledger already admitted. The fold calls it
//! `invitation_not_open`, which is true but says nothing about the file the
//! caller passed, so this module reports the `Replay error:` envelope of
//! `contracts/cli/errors.json` and exits 50.

use std::io::Write;
use std::path::Path;

use mabel_core::artifacts::{AcceptanceFile, InvitationBundle, InvitationSummary};
use mabel_core::fold::{InvitationStatus, LedgerState};
use mabel_core::sign::{
    build_acceptance, build_membership_acceptance, build_membership_invitation,
    build_membership_removal,
};
use mabel_core::{EventId, IdentityId, LedgerId};
use mabel_proto::prost::Message;
use mabel_proto::v0 as pb;
use mabel_proto::v0::event_body::Payload;

use crate::append::{append, ensure_fresh};
use crate::artifacts;
use crate::cli::{AppendOptions, RoleArg};
use crate::context::Context;
use crate::documents::{
    AcceptSurface, Accepted, Admitted, InvitationEntry, Invited, Membership, PrincipalEntry,
    Removed, RoleName, RootName, StatusName,
};
use crate::error::{CliError, Result};
use crate::ids;
use crate::ledger::Loaded;
use crate::render::Outcome;

/// `mabel membership invite --ledger <l> --by <c> --invitee <file> --role <r>
/// --out <file>`.
///
/// The invitation embeds the inception the descriptor carries, byte for byte,
/// which is what proves the invitee's id and key belong together (proposal 002
/// section 8). The bundle is written after the event lands, so it holds the
/// ledger the invitee will fold.
pub fn invite(
    ctx: &Context,
    ledger: &str,
    by: &str,
    invitee: &Path,
    role: RoleArg,
    out: &Path,
    options: &AppendOptions,
) -> Result<Outcome> {
    let ledger = ctx.resolve_local_hinted(ledger, "--ledger")?;
    let signer = ctx.resolve_local_hinted(by, "--by")?;
    let descriptor = artifacts::read_identity_descriptor(invitee)?;
    let invitee_key = descriptor.active_key().ok_or_else(|| {
        CliError::policy(
            "invitee_holds_no_key",
            format!(
                "{} is an identity-rooted ledger and holds no key of its own, so it cannot be invited",
                descriptor.identity()
            ),
        )
        .with_detail("invitee", descriptor.identity().to_string())
        .with_detail("path", invitee.display().to_string())
    })?;
    let invited = descriptor.identity();

    ensure_fresh(ctx, ledger, options)?;
    let mut loaded = ctx.load(ledger)?;
    let appended = append(ctx, signer, &mut loaded, |key, at, timestamp_ms| {
        build_membership_invitation(
            key,
            at,
            invited,
            &invitee_key,
            role.proto(),
            descriptor.inception(),
            timestamp_ms,
        )
    })?;

    let events: Vec<Vec<u8>> = ctx
        .store(ledger)
        .read_all()?
        .into_iter()
        .map(|stored| stored.bytes)
        .collect();
    let bundle = InvitationBundle::new(events)
        .map_err(|error| artifacts::failure(artifacts::Kind::InvitationBundle, &error, out))?;
    let encoded = bundle.write();
    let event_count = bundle.events().len() as u64;
    let bytes = artifacts::write(out, &encoded)?;

    let role = role_name(role.proto())?;
    let document = Invited {
        ledger_id: ids::identity(ledger),
        by: ids::identity(signer),
        invitee: ids::identity(invited),
        invitee_key: ids::key(&invitee_key),
        role,
        invitation_event: ids::event(appended.event_id),
        invitation_seq: appended.seq,
        timestamp_ms: appended.timestamp_ms,
        head_seq: appended.seq,
        head_event: ids::event(appended.event_id),
        path: out.display().to_string(),
        bytes,
        event_count,
    };
    let text = format!(
        "invited {invited} as {} at seq {} of {ledger}\nwrote {} ({event_count} events, {bytes} bytes)",
        role.as_str(),
        appended.seq,
        out.display()
    );
    Outcome::new(&document, text)
}

/// `mabel membership accept <bundle> --as <alias|id> --out <file> [--yes]`.
///
/// The bundle is folded from its inception before anything is signed: the
/// summary a person confirms is the fold's answer, not the file's claim
/// (proposal 002 section 4).
pub fn accept(
    ctx: &Context,
    bundle: &Path,
    name: &str,
    out: &Path,
    yes: bool,
    json: bool,
) -> Result<Outcome> {
    let identity = ctx.resolve_local_hinted(name, "--identity")?;
    let read = artifacts::read_invitation_bundle(bundle)?;
    let summary = read
        .summary()
        .map_err(|error| artifacts::failure(artifacts::Kind::InvitationBundle, &error, bundle))?;

    if summary.invitee != identity {
        return Err(CliError::usage(
            "not_the_invitee",
            format!(
                "this invitation invites {}, not {identity}",
                summary.invitee
            ),
        )
        .with_detail("ledger_id", summary.ledger.to_string())
        .with_detail("invitee", summary.invitee.to_string())
        .with_detail("path", bundle.display().to_string()));
    }
    let key = ctx.signing_key(identity)?;
    if key.public() != summary.invitee_key {
        return Err(CliError::policy(
            "acceptance_invitee_key_mismatch",
            format!(
                "the invitation records key {} for {identity}, and this home signs with {}",
                summary.invitee_key,
                key.public()
            ),
        )
        .with_detail("ledger_id", summary.ledger.to_string())
        .with_detail("path", bundle.display().to_string()));
    }

    let surface = surface(&summary)?;
    // The person sees the surface before the key is used, whether or not they
    // are asked to confirm.
    if !json {
        println!("{}", surface_text(&summary, &surface));
    }
    if !yes {
        if json {
            return Err(CliError::usage(
                "confirmation_required",
                "membership accept needs --yes when --json is set",
            )
            .with_detail("ledger_id", summary.ledger.to_string()));
        }
        confirm(ctx, identity, &summary)?;
    }

    let signed = build_acceptance(&key, summary.ledger, summary.invitation_event, identity);
    let file = AcceptanceFile::new(&signed)
        .map_err(|error| artifacts::failure(artifacts::Kind::AcceptanceFile, &error, out))?;
    let bytes = artifacts::write(out, &file.write())?;

    let text = format!(
        "signed acceptance of {} as {identity}\nwrote {} ({bytes} bytes)\nhand it to a controller of {} to run mabel membership admit",
        summary.invitation_event,
        out.display(),
        summary.ledger
    );
    let document = Accepted {
        surface,
        path: out.display().to_string(),
        bytes,
    };
    Outcome::new(&document, text)
}

/// `mabel membership admit --ledger <l> --by <c> <acceptance-file>`.
pub fn admit(
    ctx: &Context,
    ledger: &str,
    by: &str,
    path: &Path,
    options: &AppendOptions,
) -> Result<Outcome> {
    let ledger = ctx.resolve_local_hinted(ledger, "--ledger")?;
    let signer = ctx.resolve_local_hinted(by, "--by")?;
    let file = artifacts::read_acceptance_file(path)?;
    ensure_fresh(ctx, ledger, options)?;
    let mut loaded = ctx.load(ledger)?;
    refuse_replay(ctx, &loaded, file.invitation_event(), path)?;

    // The invitation is what the acceptance admits (proposal 002 section 4),
    // so the role and key reported below are read from it, before the append
    // marks it accepted.
    let invitation = loaded.state.invitation(&file.invitation_event()).copied();
    let detached = file.detached();
    let appended = append(ctx, signer, &mut loaded, |key, at, timestamp_ms| {
        build_membership_acceptance(key, at, &detached, timestamp_ms)
    })?;
    let Some(invitation) = invitation else {
        // The fold rejects an acceptance naming no invitation, so this is
        // unreachable.
        return Err(CliError::internal(
            "invitation_not_folded",
            format!(
                "invitation {} is not in ledger {ledger}",
                file.invitation_event()
            ),
        ));
    };

    let role = role_name(invitation.role)?;
    let document = Admitted {
        ledger_id: ids::identity(ledger),
        by: ids::identity(signer),
        invitee: ids::identity(invitation.invitee),
        invitee_key: ids::key(&invitation.invitee_key),
        role,
        invitation_event: ids::event(file.invitation_event()),
        acceptance_event: ids::event(appended.event_id),
        acceptance_seq: appended.seq,
        timestamp_ms: appended.timestamp_ms,
        head_seq: appended.seq,
        head_event: ids::event(appended.event_id),
        path: path.display().to_string(),
    };
    let text = format!(
        "admitted {} as {} at seq {} of {ledger}",
        invitation.invitee,
        role.as_str(),
        appended.seq
    );
    Outcome::new(&document, text)
}

/// `mabel membership remove --ledger <l> --by <c> --member <alias|id>`.
///
/// One removal cancels an open invitation and takes away a principal,
/// whichever exist. The raw root and the last controller are the fold's to
/// refuse.
pub fn remove(
    ctx: &Context,
    ledger: &str,
    by: &str,
    member: &str,
    options: &AppendOptions,
) -> Result<Outcome> {
    let ledger = ctx.resolve_local_hinted(ledger, "--ledger")?;
    let signer = ctx.resolve_local_hinted(by, "--by")?;
    let target = ctx.resolve(member)?;
    ensure_fresh(ctx, ledger, options)?;
    let mut loaded = ctx.load(ledger)?;
    let principal_removed = loaded.state.principal(&target).is_some();
    let cancelled = open_invitation(&loaded.state, target);

    let appended = append(ctx, signer, &mut loaded, |key, at, timestamp_ms| {
        build_membership_removal(key, at, target, timestamp_ms)
    })?;

    let document = Removed {
        ledger_id: ids::identity(ledger),
        by: ids::identity(signer),
        target: ids::identity(target),
        principal_removed,
        invitation_cancelled: cancelled.map(ids::event),
        removal_event: ids::event(appended.event_id),
        removal_seq: appended.seq,
        timestamp_ms: appended.timestamp_ms,
        head_seq: appended.seq,
        head_event: ids::event(appended.event_id),
    };
    let mut text = format!("removed {target} at seq {} of {ledger}", appended.seq);
    if let Some(cancelled) = cancelled {
        text.push_str(&format!("\ncancelled open invitation {cancelled}"));
    }
    Outcome::new(&document, text)
}

/// `mabel membership list --ledger <alias|id>`.
pub fn list(ctx: &Context, ledger: &str) -> Result<Outcome> {
    let ledger = ctx.resolve(ledger)?;
    let loaded = ctx.load(ledger)?;
    let root = root_name(&loaded)?;
    let principals = principals(&loaded.state);
    let mut invitations = Vec::new();
    for (event, invitation) in loaded.state.invitations() {
        invitations.push(InvitationEntry {
            invitation_event: ids::event(*event),
            invitation_seq: loaded.seq_of.get(event).copied().unwrap_or_default(),
            invitee: ids::identity(invitation.invitee),
            invitee_key: ids::key(&invitation.invitee_key),
            role: role_name(invitation.role)?,
            status: StatusName::of(invitation.status),
        });
    }
    invitations.sort_by_key(|entry| entry.invitation_seq);

    let open = invitations
        .iter()
        .filter(|entry| entry.status == StatusName::Open)
        .count();
    let mut text = format!(
        "{ledger}: {} principals, {open} open invitations up to seq {}",
        principals.len(),
        loaded.head_seq
    );
    for principal in &principals {
        text.push_str(&format!(
            "\n{} {} ({}){}",
            principal.role.as_str(),
            principal.identity,
            principal.active_key,
            if principal.is_root { " root" } else { "" }
        ));
    }
    for invitation in &invitations {
        text.push_str(&format!(
            "\ninvitation {} at seq {} offers {} to {}, {}",
            invitation.invitation_event,
            invitation.invitation_seq,
            invitation.role.as_str(),
            invitation.invitee,
            invitation.status.as_str()
        ));
    }

    let document = Membership {
        ledger_id: ids::identity(ledger),
        declared_kind: loaded.declared_kind(),
        root,
        head_seq: loaded.head_seq,
        head_event: ids::event(loaded.head_event),
        principals,
        invitations,
    };
    Outcome::new(&document, text)
}

/// The accept surface of proposal 002 section 4, as a document.
fn surface(summary: &InvitationSummary) -> Result<AcceptSurface> {
    let mut controllers = Vec::new();
    for principal in &summary.controllers {
        controllers.push(PrincipalEntry {
            identity: ids::identity(principal.identity),
            active_key: ids::key(&principal.key),
            role: RoleName::Controller,
            is_root: Some(principal.identity) == root_identity(summary),
        });
    }
    let warning = summary.controller_on_raw_root().then(|| {
        format!(
            "accepting a controller role on a raw-rooted ledger means signing as {}: \
             every event you append to it is that identity's own event",
            summary.ledger
        )
    });
    Ok(AcceptSurface {
        ledger_id: ids::identity(summary.ledger),
        declared_kind: declared_kind(summary),
        root: RootName::of(summary.root),
        controllers,
        invitation_event: ids::event(summary.invitation_event),
        invitee: ids::identity(summary.invitee),
        invitee_key: ids::key(&summary.invitee_key),
        role: role_name(summary.role)?,
        controller_on_raw_root: summary.controller_on_raw_root(),
        warning,
    })
}

/// The accept surface as a person reads it.
fn surface_text(summary: &InvitationSummary, surface: &AcceptSurface) -> String {
    let mut text = format!(
        "invitation to {}\ndeclared kind {}, {} root\nrole offered {}",
        summary.ledger,
        surface.declared_kind,
        surface.root.as_str(),
        surface.role.as_str()
    );
    if surface.controllers.is_empty() {
        text.push_str("\nno controllers");
    }
    for controller in &surface.controllers {
        text.push_str(&format!(
            "\ncontroller {} ({})",
            controller.identity, controller.active_key
        ));
    }
    if let Some(warning) = &surface.warning {
        text.push_str(&format!("\nwarning: {warning}"));
    }
    text
}

/// Asks the person at the terminal, after the surface has been printed.
///
/// # Errors
///
/// Returns code 2 when the answer is not `yes`, having signed nothing.
fn confirm(ctx: &Context, identity: IdentityId, summary: &InvitationSummary) -> Result<()> {
    print!(
        "accept as {} ({identity})? type yes to sign: ",
        ctx.alias(identity)
    );
    let _ = std::io::stdout().flush();
    let mut answer = String::new();
    std::io::stdin()
        .read_line(&mut answer)
        .map_err(|error| CliError::internal("io_error", format!("cannot read stdin: {error}")))?;
    if matches!(answer.trim().to_ascii_lowercase().as_str(), "yes" | "y") {
        return Ok(());
    }
    Err(
        CliError::usage("not_confirmed", "not confirmed; nothing was signed")
            .with_detail("ledger_id", summary.ledger.to_string()),
    )
}

/// Refuses an acceptance this ledger already admitted (pitfall 4).
///
/// # Errors
///
/// Returns code 50, `Replay error:`, with reason `acceptance_already_used`.
fn refuse_replay(ctx: &Context, loaded: &Loaded, invitation: EventId, path: &Path) -> Result<()> {
    let Some(held) = loaded.state.invitation(&invitation) else {
        return Ok(());
    };
    if held.status != InvitationStatus::Accepted {
        return Ok(());
    }
    let at_seq = admitted_at(ctx, loaded.ledger, invitation)?.unwrap_or_default();
    Err(CliError::new(
        mabel_node::api::ErrorLayer::Replay,
        "acceptance_already_used",
        format!(
            "this acceptance was already admitted at seq {at_seq} of {}",
            loaded.ledger
        ),
    )
    .with_detail("ledger_id", loaded.ledger.to_string())
    .with_detail("invitation_event", invitation.to_string())
    .with_detail("at_seq", at_seq)
    .with_detail("path", path.display().to_string()))
}

/// The position of the acceptance that consumed `invitation`.
///
/// The fold records that an invitation was accepted but not which event
/// accepted it, so the stored events are scanned for the acceptance whose blob
/// names it.
fn admitted_at(ctx: &Context, ledger: LedgerId, invitation: EventId) -> Result<Option<u64>> {
    for stored in ctx.store(ledger).read_all()? {
        let Some(payload) = payload_of(&stored.bytes) else {
            continue;
        };
        let Payload::MembershipAcceptance(acceptance) = payload else {
            continue;
        };
        let Ok(blob) = pb::Acceptance::decode(&acceptance.acceptance[..]) else {
            continue;
        };
        if EventId::from_slice(&blob.invitation_event) == Ok(invitation) {
            return Ok(Some(stored.seq));
        }
    }
    Ok(None)
}

/// The payload of stored event bytes the fold has already accepted.
fn payload_of(bytes: &[u8]) -> Option<Payload> {
    pb::SignedEvent::decode(bytes)
        .ok()
        .and_then(|signed| pb::EventBody::decode(&signed.body[..]).ok())
        .and_then(|body| body.payload)
}

/// The open invitation of `target`, if the ledger holds one.
fn open_invitation(state: &LedgerState, target: IdentityId) -> Option<EventId> {
    state
        .invitations()
        .iter()
        .find(|(_, invitation)| {
            invitation.invitee == target && invitation.status == InvitationStatus::Open
        })
        .map(|(event, _)| *event)
}

/// Every principal a ledger records, by ascending identity id.
fn principals(state: &LedgerState) -> Vec<PrincipalEntry> {
    let root = state.root_identity();
    state
        .principals()
        .iter()
        .filter_map(|(identity, principal)| {
            Some(PrincipalEntry {
                identity: ids::identity(*identity),
                active_key: ids::key(&principal.active_key),
                role: RoleName::of(principal.role)?,
                is_root: Some(*identity) == root,
            })
        })
        .collect()
}

fn root_identity(summary: &InvitationSummary) -> Option<IdentityId> {
    match summary.root {
        mabel_core::fold::LedgerRoot::Raw { .. } => Some(summary.ledger),
        mabel_core::fold::LedgerRoot::Identity { founder, .. } => Some(founder),
    }
}

fn declared_kind(summary: &InvitationSummary) -> mabel_node::api::documents::DeclaredKind {
    mabel_node::api::documents::DeclaredKind::parse(mabel_core::declared_kind_name(
        summary.declared_kind,
    ))
    .unwrap_or(mabel_node::api::documents::DeclaredKind::Person)
}

fn root_name(loaded: &Loaded) -> Result<RootName> {
    loaded.state.root().map(RootName::of).ok_or_else(|| {
        CliError::internal(
            "no_root",
            format!("ledger {} holds no inception", loaded.ledger),
        )
    })
}

/// The name of a role the fold recorded.
///
/// The fold never records `ROLE_UNSPECIFIED`, which the field table rejects,
/// so this cannot fail on a stored ledger.
fn role_name(role: pb::Role) -> Result<RoleName> {
    RoleName::of(role).ok_or_else(|| {
        CliError::internal(
            "unspecified_role",
            "a membership event carries no recognised role",
        )
    })
}
