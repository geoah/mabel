//! `mabel identity create|list|show|rotate`.

use mabel_core::fold::LedgerState;
use mabel_core::sign::{Root, build_inception};
use mabel_core::{IdentityId, NONCE_BYTES};
use mabel_node::api::documents::Identity;
use mabel_node::keys::generate_secret_key;
use mabel_node::{IdentityMeta, LedgerMeta, now_ms};

use crate::cli::Kind;
use crate::context::Context;
use crate::documents::{CreatedIdentity, IdentityList};
use crate::error::{CliError, Result};
use crate::ids;
use crate::ledger::Loaded;
use crate::render::Outcome;

/// `mabel identity create --alias <a> [--kind <k>] [--founder <alias|id>]`.
///
/// Without `--founder` the identity keys itself: the inception carries a raw
/// root, the active key signs it and the reserve key is committed to but never
/// recorded. With `--founder` the inception carries an identity root, the
/// founder's active key signs it, and the new ledger holds no key of its own
/// (proposal 002 section 2).
pub fn create(ctx: &Context, alias: &str, kind: Kind, founder: Option<&str>) -> Result<Outcome> {
    refuse_reused_alias(ctx, alias)?;
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
    Outcome::new(&document, text)
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
        identities.push(ctx.load(identity)?.identity_document(ctx.alias(identity)));
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
    let loaded = ctx.load(identity)?;
    let document = loaded.identity_document(ctx.alias(identity));
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
