//! The one path that adds an event to a stored ledger.
//!
//! Order matters and is the same for every command: fold what is stored,
//! refuse a chain that does not verify, build the event, run it through
//! [`LedgerState::apply`], and only then write it. An event the fold rejects
//! never reaches the disk (proposal 001 section 3.6).
//!
//! The timestamp is `max(now, prev.timestamp_ms)`, the clamp rule of section
//! 3.2, computed here so the value a command reports is the value the event
//! carries.

use iroh_base::SecretKey;
use mabel_core::sign::{BuildError, BuiltEvent, Position, ledger_timestamp_ms};
use mabel_core::{EventId, IdentityId};
use mabel_node::{NewEvent, now_ms};

use crate::context::Context;
use crate::error::{CliError, Result};
use crate::ledger::Loaded;

/// What an append produced.
#[derive(Debug, Clone, Copy)]
pub struct Appended {
    /// The new event.
    pub event_id: EventId,
    /// Its position, which is the ledger's new head sequence.
    pub seq: u64,
    /// The `timestamp_ms` it carries.
    pub timestamp_ms: u64,
}

/// Signs one event for `identity` and appends it to `loaded`.
///
/// `build` is the `mabel_core::sign` builder for the payload, handed the
/// signing key, the position and the clamped timestamp.
///
/// # Errors
///
/// Returns code 20 when the stored chain does not verify or the fold rejects
/// the new event, code 60 for an insecure key file, and the storage errors of
/// the append.
pub fn append<F>(
    ctx: &Context,
    identity: IdentityId,
    loaded: &mut Loaded,
    build: F,
) -> Result<Appended>
where
    F: FnOnce(&SecretKey, &Position, u64) -> std::result::Result<BuiltEvent, BuildError>,
{
    loaded.require_valid()?;
    let head = loaded.state.head().ok_or_else(|| {
        CliError::usage(
            "empty_ledger",
            format!("ledger {} holds no inception", loaded.ledger),
        )
    })?;
    let signer = ctx.signing_key(identity)?;
    let at = Position {
        ledger: loaded.ledger,
        seq: head.seq + 1,
        prev: head.event_id,
        prev_timestamp_ms: head.timestamp_ms,
    };
    let timestamp_ms = ledger_timestamp_ms(now_ms(), head.timestamp_ms);
    let built = build(&signer, &at, timestamp_ms)?;
    loaded
        .state
        .apply(&built.signed_event)
        .map_err(|reason| loaded.rejection(&reason, at.seq))?;
    ctx.store(loaded.ledger).append(&[NewEvent {
        seq: at.seq,
        event_id: built.event_id,
        bytes: &built.signed_event,
    }])?;
    loaded.seq_of.insert(built.event_id, at.seq);
    loaded.head_seq = at.seq;
    loaded.head_event = built.event_id;
    loaded.event_count += 1;
    Ok(Appended {
        event_id: built.event_id,
        seq: at.seq,
        timestamp_ms,
    })
}
