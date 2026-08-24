//! `mabel identity create|list|show|rotate`.

use std::path::Path;

use mabel_core::artifacts::IdentityDescriptor;
use mabel_core::fold::{LedgerRoot, LedgerState, Reason};
use mabel_core::sign::{Root, build_inception, build_profile_update, check_profile};
use mabel_core::{IdentityId, NONCE_BYTES};
use mabel_node::api::documents::Identity;
use mabel_node::keys::generate_secret_key;
use mabel_node::{IdentityMeta, LedgerMeta, now_ms};

use crate::append::append;
use crate::cli::Kind;
use crate::context::Context;
use crate::documents::{CreatedIdentity, ExportedIdentity, IdentityList, RootName};
use crate::error::{CliError, Result};
use crate::ids;
use crate::ledger::Loaded;
use crate::render::Outcome;

/// `mabel identity create --alias <a> [--kind <k>] [--founder <alias|id>]
/// [--name <display name>] [--email <email>]`.
///
/// Without `--founder` the identity keys itself: the inception carries a raw
/// root, the active key signs it and the reserve key is committed to but never
/// recorded. With `--founder` the inception carries an identity root, the
/// founder's active key signs it, and the new ledger holds no key of its own
/// (proposal 002 section 2).
///
/// `--name` or `--email` adds one `ProfileUpdate` at seq 1, so a new identity's
/// first two events are who it is and what it shows the world (proposal 005).
/// Neither given, the new ledger is one event long.
pub fn create(
    ctx: &Context,
    alias: &str,
    kind: Kind,
    founder: Option<&str>,
    name: Option<&str>,
    email: Option<&str>,
) -> Result<Outcome> {
    refuse_reused_alias(ctx, alias)?;
    let name = trimmed(name);
    let email = trimmed(email);
    // Before the mint, not after: a name or an email the scanner refuses must
    // leave no ledger and no taken alias behind.
    check_profile(name, None, email).map_err(|error| CliError::from(&Reason::Wire(error)))?;
    let mut nonce = [0u8; NONCE_BYTES];
    getrandom::fill(&mut nonce)
        .map_err(|error| CliError::internal("no_randomness", error.to_string()))?;
    let created_at_ms = now_ms();

    let founder = match founder {
        Some(name) => Some(ctx.resolve_local(name)?),
        None => None,
    };
    let (built, keys) = match founder {
        Some(founder) => {
            let signer = ctx.signing_key(founder)?;
            let inception = ctx.store(founder).read_event(0)?;
            let built = build_inception(
                &signer,
                kind.proto(),
                Root::Identity {
                    founder,
                    founder_inception: &inception,
                },
                nonce,
                created_at_ms,
            )?;
            (built, None)
        }
        None => {
            let active = generate_secret_key()?;
            let reserve = generate_secret_key()?;
            let built = build_inception(
                &active,
                kind.proto(),
                Root::Raw {
                    reserve_key: &reserve.public(),
                },
                nonce,
                created_at_ms,
            )?;
            (built, Some((active, reserve)))
        }
    };

    // The fold decides whether these bytes are a ledger before they land.
    let mut state = LedgerState::default();
    state
        .apply(&built.signed_event)
        .map_err(|reason| CliError::from(&reason))?;
    let identity = IdentityId::from(built.event_id);

    if let Some((active, reserve)) = &keys {
        ctx.home().write_identity_keys(identity, active, reserve)?;
    }
    let store = ctx.store(identity);
    store.append(&[mabel_node::NewEvent {
        seq: 0,
        event_id: built.event_id,
        bytes: &built.signed_event,
    }])?;
    store.write_meta(&LedgerMeta {
        source_endpoint: None,
        first_seen_ms: created_at_ms,
    })?;
    ctx.home().create_identity(
        identity,
        &IdentityMeta {
            alias: alias.to_owned(),
            declared_kind: kind.stored(),
            controlled_by: founder,
            created_at_ms,
        },
    )?;

    // The profile lands as its own event at seq 1: the inception stays minimal,
    // and the profile keeps the whole-replacement semantics it has everywhere
    // else (proposal 005).
    if name.is_some() || email.is_some() {
        let mut loaded = ctx.load(identity)?;
        append(ctx, identity, &mut loaded, |signer, at, timestamp_ms| {
            build_profile_update(signer, at, name, None, email, timestamp_ms)
        })?;
    }

    let loaded = ctx.load(identity)?;
    let document = CreatedIdentity {
        identity_id: ids::identity(identity),
        declared_kind: loaded.declared_kind(),
        alias: alias.to_owned(),
        active_key: keys.as_ref().map(|(active, _)| ids::key(&active.public())),
        reserve_commit: reserve_commit(&loaded),
        created_at_ms: loaded.created_at_ms,
        inception_event: ids::event(built.event_id),
        head_seq: loaded.head_seq,
        head_event: ids::event(loaded.head_event),
        profile: loaded.profile(),
        witnesses: loaded.witnesses(),
    };
    let mut text = format!(
        "created identity {identity}\nalias {alias}, declared kind {}",
        document.declared_kind
    );
    match founder {
        Some(founder) => text.push_str(&format!(
            ", identity root\nfounding principal {founder} ({})",
            ctx.alias(founder)
        )),
        None => text.push_str(", raw root"),
    }
    if let Some(profile) = &document.profile {
        text.push_str(&format!(
            "\npublished at seq {}: display name {}, email {}",
            profile.seq,
            shown(profile.display_name.as_deref()),
            shown(profile.email.as_deref())
        ));
    }
    Outcome::new(&document, text)
}

/// An empty flag value is no value: `--name ""` publishes nothing rather than
/// an empty string, which the wire encoding cannot carry anyway.
fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn shown(value: Option<&str>) -> String {
    value.map_or_else(|| "(unset)".to_owned(), ToOwned::to_owned)
}

/// `mabel identity list`.
pub fn list(ctx: &Context) -> Result<Outcome> {
    let mut identities = Vec::new();
    for identity in ctx.home().identities()? {
        // An identity whose ledger never landed is skipped rather than
        // failing the whole listing.
        if ctx.store(identity).head()?.is_none() {
            continue;
        }
        identities.push(ctx.identity_document(identity)?);
    }
    let text = if identities.is_empty() {
        "no identities in this home".to_owned()
    } else {
        identities
            .iter()
            .map(line)
            .collect::<Vec<String>>()
            .join("\n")
    };
    Outcome::new(&IdentityList { identities }, text)
}

/// `mabel identity show <alias|id>`.
pub fn show(ctx: &Context, name: &str) -> Result<Outcome> {
    let identity = ctx.resolve(name)?;
    let document = ctx.identity_document(identity)?;
    let mut text = format!(
        "{}\nalias {}, declared kind {}\nhead seq {}, {} events",
        document.identity_id,
        document.alias,
        document.declared_kind,
        document.head_seq,
        document.event_count
    );
    if let Some(active_key) = &document.active_key {
        text.push_str(&format!("\nactive key {active_key}"));
    }
    if document.witnesses.is_empty() {
        text.push_str("\nno witnesses configured");
    } else {
        for witness in &document.witnesses {
            text.push_str(&format!("\nwitness {witness}"));
        }
    }
    for entry in &document.trust {
        text.push_str(&format!(
            "\nattestation {} at seq {} names {}",
            entry.attestation_event, entry.attestation_seq, entry.subject
        ));
    }
    Outcome::new(&document, text)
}

/// `mabel identity export <alias|id> --out <path>`.
///
/// The descriptor carries the ledger's seq-0 event bytes as they are stored and
/// the witness set the ledger currently records. An invitation embeds those
/// same inception bytes, which is what ties the invitee's id to their key
/// (proposal 001 section 3.8).
pub fn export(ctx: &Context, name: &str, out: &Path) -> Result<Outcome> {
    let identity = ctx.resolve(name)?;
    let loaded = ctx.load(identity)?;
    loaded.require_valid()?;
    let inception = ctx.store(identity).read_event(0)?;
    let witnesses = loaded.state.witnesses().to_vec();
    let descriptor = IdentityDescriptor::new(&inception, &witnesses).map_err(|error| {
        crate::artifacts::failure(crate::artifacts::Kind::IdentityDescriptor, &error, out)
    })?;
    let bytes = crate::artifacts::write(out, &descriptor.write())?;

    let root = match descriptor.root() {
        LedgerRoot::Raw { .. } => RootName::Raw,
        LedgerRoot::Identity { .. } => RootName::Identity,
    };
    let document = ExportedIdentity {
        identity_id: ids::identity(identity),
        declared_kind: loaded.declared_kind(),
        root,
        active_key: descriptor.active_key().as_ref().map(ids::key),
        witnesses: loaded.witnesses(),
        path: out.display().to_string(),
        bytes,
    };
    let text = format!(
        "exported {identity} to {} ({bytes} bytes)\ndeclared kind {}, {} root, {} witnesses",
        out.display(),
        document.declared_kind,
        root.as_str(),
        document.witnesses.len()
    );
    Outcome::new(&document, text)
}

/// `mabel identity rotate`, which this POC does not implement (decision 008,
/// proposal 002 section 9).
pub fn rotate() -> Result<Outcome> {
    Err(CliError::unsupported(
        "unsupported_feature",
        "key rotation is not part of this POC",
    )
    .with_detail("feature", "key_rotation"))
}

fn refuse_reused_alias(ctx: &Context, alias: &str) -> Result<()> {
    for identity in ctx.home().identities()? {
        if ctx.home().identity_meta(identity)?.alias == alias {
            return Err(CliError::usage(
                "alias_in_use",
                format!("{alias} already names {identity} in this home"),
            )
            .with_detail("alias", alias)
            .with_detail("identity", identity.to_string()));
        }
    }
    Ok(())
}

fn reserve_commit(loaded: &Loaded) -> Option<mabel_node::api::documents::Id> {
    match loaded.state.root()? {
        mabel_core::fold::LedgerRoot::Raw { reserve_commit, .. } => {
            Some(ids::bytes(&reserve_commit))
        }
        mabel_core::fold::LedgerRoot::Identity { .. } => None,
    }
}

fn line(identity: &Identity) -> String {
    format!(
        "{}  {}  {}  head seq {}",
        identity.identity_id, identity.alias, identity.declared_kind, identity.head_seq
    )
}
