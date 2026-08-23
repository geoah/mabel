//! `mabel profile replace`.
//!
//! The operation is replacement, and every surface says so: an omitted flag
//! clears that field, the command prints a before-and-after diff, and it asks
//! for confirmation unless `--yes` is given (proposal 003 section 1).
//!
//! A hostname is a public claim. The diff says so before anything is signed,
//! because the chain is the full history: a hostname set once stays readable
//! in every replica forever, and a later update that omits it changes only
//! what the fold reports.
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

/// `mabel profile replace --identity <alias|id> [--display-name X]
/// [--hostname Y] [--yes]`.
pub fn replace(
    ctx: &Context,
    name: &str,
    display_name: Option<&str>,
    hostname: Option<&str>,
    yes: bool,
    json: bool,
    options: &AppendOptions,
) -> Result<Outcome> {
    let identity = ctx.resolve_local(name)?;
    let display_name = trimmed(display_name);
    let hostname = trimmed(hostname);

    let previous = ctx.load(identity)?.profile();
    refuse_no_op(identity, previous.as_ref(), display_name, hostname)?;

    let diff = diff_text(ctx, identity, previous.as_ref(), display_name, hostname);
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
    refuse_no_op(identity, previous.as_ref(), display_name, hostname)?;
    let appended = append(ctx, identity, &mut loaded, |signer, at, timestamp_ms| {
        build_profile_update(signer, at, display_name, hostname, timestamp_ms)
    })?;

    let document = ReplacedProfile {
        identity_id: ids::identity(identity),
        display_name: display_name.map(ToOwned::to_owned),
        hostname: hostname.map(ToOwned::to_owned),
        previous: PreviousProfile {
            display_name: previous
                .as_ref()
                .and_then(|profile| profile.display_name.clone()),
            hostname: previous.and_then(|profile| profile.hostname),
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
        summary(display_name, hostname)
    );
    Outcome::new(&document, text)
}

/// An empty flag value is no value: `--display-name ""` clears the name
/// rather than publishing an empty string, which the wire encoding cannot
/// carry anyway.
fn trimmed(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

/// The before-and-after diff a person reads.
fn diff_text(
    ctx: &Context,
    identity: IdentityId,
    previous: Option<&Profile>,
    display_name: Option<&str>,
    hostname: Option<&str>,
) -> String {
    let before_name = previous.and_then(|profile| profile.display_name.as_deref());
    let before_host = previous.and_then(|profile| profile.hostname.as_deref());
    let mut text = format!(
        "{} ({identity})\n  display name: {} -> {}\n  hostname:     {} -> {}",
        ctx.alias(identity),
        shown(before_name),
        shown(display_name),
        shown(before_host),
        shown(hostname)
    );
    if hostname.is_some() && hostname != before_host {
        text.push_str(
            "\npublishing a hostname puts it on the ledger, where every replica keeps it \
             readable forever; a later update changes only what the fold reports",
        );
    }
    text
}

fn shown(value: Option<&str>) -> String {
    value.map_or_else(|| "(unset)".to_owned(), ToOwned::to_owned)
}

fn summary(display_name: Option<&str>, hostname: Option<&str>) -> String {
    format!(
        "display name {}, hostname {}",
        shown(display_name),
        shown(hostname)
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
    display_name: Option<&str>,
    hostname: Option<&str>,
) -> Result<()> {
    let current = (
        previous.and_then(|profile| profile.display_name.as_deref()),
        previous.and_then(|profile| profile.hostname.as_deref()),
    );
    if current != (display_name, hostname) {
        return Ok(());
    }
    let mut error = CliError::policy(
        "no_op_profile_update",
        format!("this profile is already the profile of {identity}: nothing would change"),
    )
    .with_detail("ledger_id", identity.to_string())
    .with_detail("display_name", display_name)
    .with_detail("hostname", hostname);
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
