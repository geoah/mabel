//! `mabel profile replace`.
//!
//! The operation is replacement, and every surface says so: an omitted flag
//! clears that field, the command prints a before-and-after diff, and it asks
//! for confirmation unless `--yes` is given (proposal 003 section 1).
//!
//! A hostname and an email are public claims. The diff says so before anything
//! is signed, because the chain is the full history: a claim published once
//! stays readable in every replica forever, and a later update that omits it
//! changes only what the fold reports.
//!
//! The no-op guard lives in `mabel-node`, so `mabel profile replace` and
//! `POST /api/identities/:identity_id/profile` refuse the same event with the
//! same `no_op_profile_update` envelope and the same exit code 20.

use std::io::Write;

use mabel_core::IdentityId;
use mabel_core::sign::build_profile_update;
use mabel_node::api::documents::Profile;

use crate::append::{append, ensure_fresh};
use crate::cli::AppendOptions;
use crate::context::Context;
use crate::documents::{PreviousProfile, ReplacedProfile};
use crate::error::{CliError, Result};
use crate::ids;
use crate::render::Outcome;

/// The whole profile one replacement publishes.
///
/// One value, because the payload is one document: a field left `None` is a
/// field the update clears, and nothing may pass two of the three and forget
/// the last (proposal 003 section 1, proposal 005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fields<'a> {
    /// The name to publish.
    pub display_name: Option<&'a str>,
    /// The hostname to claim.
    pub hostname: Option<&'a str>,
    /// The email to publish.
    pub email: Option<&'a str>,
}

impl<'a> Fields<'a> {
    /// The flags as they arrived, with an empty value read as no value:
    /// `--display-name ""` clears the name rather than publishing an empty
    /// string, which the wire encoding cannot carry anyway.
    #[must_use]
    pub fn new(
        display_name: Option<&'a str>,
        hostname: Option<&'a str>,
        email: Option<&'a str>,
    ) -> Self {
        Self {
            display_name: trimmed(display_name),
            hostname: trimmed(hostname),
            email: trimmed(email),
        }
    }

    /// What the fold reports for a profile, or three `None` for a ledger that
    /// carries none.
    fn of(profile: Option<&'a Profile>) -> Self {
        Self {
            display_name: profile.and_then(|profile| profile.display_name.as_deref()),
            hostname: profile.and_then(|profile| profile.hostname.as_deref()),
            email: profile.and_then(|profile| profile.email.as_deref()),
        }
    }
}

/// `mabel profile replace --identity <alias|id> [--display-name X]
/// [--hostname Y] [--email Z] [--yes]`.
pub fn replace(
    ctx: &Context,
    name: &str,
    fields: Fields<'_>,
    yes: bool,
    json: bool,
    options: &AppendOptions,
) -> Result<Outcome> {
    let identity = ctx.resolve_local_hinted(name, "--identity")?;

    let previous = ctx.load(identity)?.profile();
    refuse_no_op(identity, previous.as_ref(), fields)?;

    let diff = diff_text(ctx, identity, previous.as_ref(), fields);
    // The person sees what changes before the key is used, confirmed or not.
    if !json {
        println!("{diff}");
    }
    if !yes {
        if json {
            return Err(CliError::usage(
                "confirmation_required",
                "profile replace needs --yes when --json is set",
            )
            .with_detail("identity", identity.to_string()));
        }
        confirm(ctx, identity)?;
    }

    ensure_fresh(ctx, identity, options)?;
    let mut loaded = ctx.load(identity)?;
    // The head may have moved while the person was reading the diff, so the
    // guard runs once more on the chain that is actually being signed on.
    let previous = loaded.profile();
    refuse_no_op(identity, previous.as_ref(), fields)?;
    let appended = append(ctx, identity, &mut loaded, |signer, at, timestamp_ms| {
        build_profile_update(
            signer,
            at,
            fields.display_name,
            fields.hostname,
            fields.email,
            timestamp_ms,
        )
    })?;

    let document = ReplacedProfile {
        identity_id: ids::identity(identity),
        display_name: fields.display_name.map(ToOwned::to_owned),
        hostname: fields.hostname.map(ToOwned::to_owned),
        email: fields.email.map(ToOwned::to_owned),
        previous: PreviousProfile {
            display_name: previous
                .as_ref()
                .and_then(|profile| profile.display_name.clone()),
            hostname: previous
                .as_ref()
                .and_then(|profile| profile.hostname.clone()),
            email: previous.and_then(|profile| profile.email),
        },
        profile_event: ids::event(appended.event_id),
        profile_seq: appended.seq,
        timestamp_ms: appended.timestamp_ms,
        head_seq: appended.seq,
        head_event: ids::event(appended.event_id),
    };
    let text = format!(
        "replaced the profile of {identity} at seq {}\n{}",
        appended.seq,
        summary(fields)
    );
    Outcome::new(&document, text)
}

fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// The before-and-after diff a person reads.
fn diff_text(
    ctx: &Context,
    identity: IdentityId,
    previous: Option<&Profile>,
    fields: Fields<'_>,
) -> String {
    let before = Fields::of(previous);
    let mut text = format!(
        "{} ({identity})\n  display name: {} -> {}\n  hostname:     {} -> {}\n  \
         email:        {} -> {}",
        ctx.alias(identity),
        shown(before.display_name),
        shown(fields.display_name),
        shown(before.hostname),
        shown(fields.hostname),
        shown(before.email),
        shown(fields.email)
    );
    let claim = match (
        fields.hostname.is_some() && fields.hostname != before.hostname,
        fields.email.is_some() && fields.email != before.email,
    ) {
        (true, true) => Some("a hostname and an email"),
        (true, false) => Some("a hostname"),
        (false, true) => Some("an email"),
        (false, false) => None,
    };
    if let Some(claim) = claim {
        text.push_str(&format!(
            "\npublishing {claim} puts it on the ledger, where every replica keeps it \
             readable forever; a later update changes only what the fold reports"
        ));
    }
    text
}

fn shown(value: Option<&str>) -> String {
    value.map_or_else(|| "(unset)".to_owned(), ToOwned::to_owned)
}

fn summary(fields: Fields<'_>) -> String {
    format!(
        "display name {}, hostname {}, email {}",
        shown(fields.display_name),
        shown(fields.hostname),
        shown(fields.email)
    )
}

/// Refuses a replacement that would leave the profile exactly as it is.
///
/// The fold accepts a no-op update, because the fold must accept whatever a
/// valid chain contains. The node refuses to sign one, because an event that
/// says nothing is still an event every replica keeps forever.
fn refuse_no_op(
    identity: IdentityId,
    previous: Option<&Profile>,
    fields: Fields<'_>,
) -> Result<()> {
    if Fields::of(previous) != fields {
        return Ok(());
    }
    let mut error = CliError::policy(
        "no_op_profile_update",
        format!("this profile is already the profile of {identity}: nothing would change"),
    )
    .with_detail("ledger_id", identity.to_string())
    .with_detail("display_name", fields.display_name)
    .with_detail("hostname", fields.hostname)
    .with_detail("email", fields.email);
    if let Some(profile) = previous {
        error = error
            .with_detail("profile_event", profile.event.as_str())
            .with_detail("profile_seq", profile.seq);
    }
    Err(error)
}

/// Asks before the key is used.
fn confirm(ctx: &Context, identity: IdentityId) -> Result<()> {
    print!(
        "replace the profile of {} ({identity})? type yes to sign: ",
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
            .with_detail("identity", identity.to_string()),
    )
}
