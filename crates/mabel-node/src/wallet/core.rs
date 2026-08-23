//! What a wallet does with the home on disk: read identities, fold ledgers,
//! sign events and store what a peer served.
//!
//! Every append follows one order, the order of proposal 001 section 3.6:
//! fold what is stored, refuse a chain that does not verify, build the event,
//! run it through [`LedgerState::apply`], and only then write it. An event the
//! fold rejects never reaches the disk.
//!
//! Nothing here touches the network. The sync and verification paths
//! ([`crate::wallet::sync`], [`crate::wallet::verify`]) call into this type,
//! and so does the HTTP service, so one body of rules covers both surfaces.

use std::fs;

use iroh_base::{EndpointId, SecretKey};
use mabel_core::fold::LedgerState;
use mabel_core::sign::{
    BuildError, BuiltEvent, Position, Root, build_inception, build_trust_attestation,
    build_trust_revocation, build_witness_config, ledger_timestamp_ms,
};
use mabel_core::{EventId, IdentityId, LedgerId, NONCE_BYTES};

use crate::api::documents::{
    Appended, CreatedIdentity, DeclaredKind as DocumentKind, Identity, LedgerPage, Revoked,
};
use crate::api::error::ServiceError;
use crate::api::service::EventPageRequest;
use crate::config::NodeConfig;
use crate::home::{DeclaredKind, IdentityMeta, NodeHome};
use crate::ledger::{LedgerMeta, LedgerStore, NewEvent};
use crate::now_ms;
use crate::wallet::error::{build_error, fold_error, storage_error};
use crate::wallet::ids;
use crate::wallet::ledger::LoadedLedger;
use crate::witness::events::event_document;

/// What one append produced.
#[derive(Debug, Clone)]
pub struct AppendedEvent {
    /// The new event.
    pub event_id: EventId,
    /// Its position, which is the ledger's new head sequence.
    pub seq: u64,
    /// The `timestamp_ms` it carries.
    pub timestamp_ms: u64,
    /// The encoded `SignedEvent` that landed.
    pub bytes: Vec<u8>,
}

/// The wallet's view of one node home.
#[derive(Debug, Clone)]
pub struct WalletCore {
    home: NodeHome,
}

impl WalletCore {
    /// A wallet over `home`.
    #[must_use]
    pub fn new(home: NodeHome) -> Self {
        Self { home }
    }

    /// The home this wallet reads and writes.
    #[must_use]
    pub fn home(&self) -> &NodeHome {
        &self.home
    }

    /// `node.json`.
    ///
    /// # Errors
    ///
    /// Returns code 10 for a malformed file.
    pub fn config(&self) -> Result<NodeConfig, ServiceError> {
        self.home.config().map_err(storage_error)
    }

    /// This node's Iroh endpoint id, which is the source a local verification
    /// reports (flag R, proposal 001 section 6).
    ///
    /// # Errors
    ///
    /// Returns code 60 for a group- or world-accessible `node.key`.
    pub fn endpoint_id(&self) -> Result<EndpointId, ServiceError> {
        Ok(self.node_key()?.public())
    }

    /// The Iroh endpoint secret key.
    ///
    /// # Errors
    ///
    /// As [`WalletCore::endpoint_id`].
    pub fn node_key(&self) -> Result<SecretKey, ServiceError> {
        self.home.node_key().map_err(storage_error)
    }

    /// The store for one ledger.
    #[must_use]
    pub fn store(&self, ledger: LedgerId) -> LedgerStore {
        self.home.ledger(ledger)
    }

    /// Whether this home holds any event of `ledger`.
    ///
    /// # Errors
    ///
    /// Returns the storage errors of reading the head cache.
    pub fn holds(&self, ledger: LedgerId) -> Result<bool, ServiceError> {
        Ok(self.store(ledger).head().map_err(storage_error)?.is_some())
    }

    /// Folds one stored ledger.
    ///
    /// # Errors
    ///
    /// Returns code 2 with reason `unknown_ledger` when the home holds none of
    /// it, and the storage errors of reading the event files.
    pub fn load(&self, ledger: LedgerId) -> Result<LoadedLedger, ServiceError> {
        let loaded = LoadedLedger::open(&self.store(ledger))?;
        if loaded.is_empty() {
            return Err(unknown_ledger(ledger));
        }
        Ok(loaded)
    }

    /// The alias recorded for an identity, or its id when the home records
    /// none.
    #[must_use]
    pub fn alias(&self, identity: IdentityId) -> String {
        self.home
            .identity_meta(identity)
            .map_or_else(|_| identity.to_string(), |meta| meta.alias)
    }

    /// Every identity this home holds a ledger for, by ascending id.
    ///
    /// # Errors
    ///
    /// Returns the storage errors of listing `identities/`.
    pub fn identities(&self) -> Result<Vec<Identity>, ServiceError> {
        let mut identities = Vec::new();
        for identity in self.home.identities().map_err(storage_error)? {
            // An identity whose ledger never landed is skipped rather than
            // failing the whole listing.
            if !self.holds(identity)? {
                continue;
            }
            identities.push(self.load(identity)?.identity_document(self.alias(identity)));
        }
        Ok(identities)
    }

    /// One identity document.
    ///
    /// # Errors
    ///
    /// As [`WalletCore::load`].
    pub fn identity(&self, identity: IdentityId) -> Result<Identity, ServiceError> {
        Ok(self.load(identity)?.identity_document(self.alias(identity)))
    }

    /// One page of an identity's ledger.
    ///
    /// # Errors
    ///
    /// As [`WalletCore::load`], plus code 10 when a stored event does not
    /// decode.
    pub fn identity_ledger(
        &self,
        identity: IdentityId,
        page: EventPageRequest,
    ) -> Result<LedgerPage, ServiceError> {
        let loaded = self.load(identity)?;
        let last = page.since.saturating_add(u64::from(page.limit));
        let mut events = Vec::new();
        for (index, bytes) in loaded.events.iter().enumerate() {
            let seq = index as u64;
            if seq < page.since || seq >= last {
                continue;
            }
            events.push(event_document(bytes)?);
        }
        Ok(LedgerPage {
            ledger_id: ids::identity(identity),
            declared_kind: loaded.declared_kind(),
            since: page.since,
            limit: page.limit,
            head_seq: loaded.head_seq,
            head_event: ids::event(loaded.head_event),
            event_count: loaded.event_count(),
            more: last < loaded.event_count(),
            events,
        })
    }

    /// Mints a raw-rooted identity: a fresh active key, a reserve commitment
    /// and a seq-0 event (proposal 002 section 2).
    ///
    /// # Errors
    ///
    /// Returns code 2 when the alias is already taken here, code 10 when the
    /// inception does not build, and the storage errors of writing the keys
    /// and the event.
    pub fn create_identity(
        &self,
        alias: &str,
        declared_kind: DocumentKind,
    ) -> Result<CreatedIdentity, ServiceError> {
        self.refuse_reused_alias(alias)?;
        let mut nonce = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|error| {
            ServiceError::state("no_randomness", error.to_string())
                .with_status(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
        let created_at_ms = now_ms();

        let active = crate::keys::generate_secret_key().map_err(storage_error)?;
        let reserve = crate::keys::generate_secret_key().map_err(storage_error)?;
        let built = build_inception(
            &active,
            proto_kind(declared_kind),
            Root::Raw {
                reserve_key: &reserve.public(),
            },
            nonce,
            created_at_ms,
        )
        .map_err(|error| build_error(&error))?;

        // The fold decides whether these bytes are a ledger before they land.
        let mut state = LedgerState::default();
        state
            .apply(&built.signed_event)
            .map_err(|reason| fold_error(&reason))?;
        let identity = IdentityId::from(built.event_id);

        self.home
            .write_identity_keys(identity, &active, &reserve)
            .map_err(storage_error)?;
        let store = self.store(identity);
        store
            .append(&[NewEvent {
                seq: 0,
                event_id: built.event_id,
                bytes: &built.signed_event,
            }])
            .map_err(storage_error)?;
        store
            .write_meta(&LedgerMeta {
                source_endpoint: None,
                first_seen_ms: created_at_ms,
            })
            .map_err(storage_error)?;
        self.home
            .create_identity(
                identity,
                &IdentityMeta {
                    alias: alias.to_owned(),
                    declared_kind: stored_kind(declared_kind),
                    controlled_by: None,
                    created_at_ms,
                },
            )
            .map_err(storage_error)?;

        Ok(CreatedIdentity {
            identity: self.identity(identity)?,
            inception_event: ids::event(built.event_id),
        })
    }

    /// Appends a `WitnessConfig` recording `witnesses`, replacing the whole
    /// set (proposal 001 section 3.4).
    ///
    /// # Errors
    ///
    /// As [`WalletCore::append`].
    pub fn set_witnesses(
        &self,
        identity: IdentityId,
        witnesses: &[EndpointId],
    ) -> Result<Appended, ServiceError> {
        let mut loaded = self.load(identity)?;
        let appended = self.append(identity, &mut loaded, |signer, at, timestamp_ms| {
            build_witness_config(signer, at, witnesses, timestamp_ms)
        })?;
        self.appended_document(identity, &appended)
    }

    /// Appends a `TrustAttestation` naming `subject`.
    ///
    /// # Errors
    ///
    /// As [`WalletCore::append`].
    pub fn add_trust(
        &self,
        issuer: IdentityId,
        subject: IdentityId,
    ) -> Result<Appended, ServiceError> {
        let mut loaded = self.load(issuer)?;
        let appended = self.append(issuer, &mut loaded, |signer, at, timestamp_ms| {
            build_trust_attestation(signer, at, subject, timestamp_ms)
        })?;
        self.appended_document(issuer, &appended)
    }

    /// Appends a `TrustRevocation` naming an earlier attestation.
    ///
    /// # Errors
    ///
    /// As [`WalletCore::append`], plus code 20 when the ledger holds no such
    /// attestation.
    pub fn revoke_trust(
        &self,
        issuer: IdentityId,
        attestation: EventId,
    ) -> Result<Revoked, ServiceError> {
        let mut loaded = self.load(issuer)?;
        let attestation_seq = loaded.seq_of.get(&attestation).copied();
        let appended = self.append(issuer, &mut loaded, |signer, at, timestamp_ms| {
            build_trust_revocation(signer, at, attestation, timestamp_ms)
        })?;
        let Some(attestation_seq) = attestation_seq else {
            // The fold rejects a target it does not hold, so this is
            // unreachable.
            return Err(ServiceError::ledger(
                "attestation_not_folded",
                format!("attestation {attestation} is not in ledger {issuer}"),
            ));
        };
        Ok(Revoked {
            ledger_id: ids::identity(issuer),
            head_seq: appended.seq,
            head_event: ids::event(appended.event_id),
            revoked_attestation: ids::event(attestation),
            revoked_attestation_seq: attestation_seq,
            event: event_document(&appended.bytes)?,
        })
    }

    /// Signs one event for `identity` and appends it to `loaded`.
    ///
    /// # Errors
    ///
    /// Returns code 20 when the stored chain does not verify or the fold
    /// rejects the new event, code 60 for an insecure key file, and the
    /// storage errors of the append.
    pub fn append<F>(
        &self,
        identity: IdentityId,
        loaded: &mut LoadedLedger,
        build: F,
    ) -> Result<AppendedEvent, ServiceError>
    where
        F: FnOnce(&SecretKey, &Position, u64) -> Result<BuiltEvent, BuildError>,
    {
        self.require_valid(loaded)?;
        let head = loaded.state.head().ok_or_else(|| {
            ServiceError::usage(
                "empty_ledger",
                format!("ledger {} holds no inception", loaded.ledger),
            )
        })?;
        let signer = self.signing_key(identity)?;
        let at = Position {
            ledger: loaded.ledger,
            seq: head.seq + 1,
            prev: head.event_id,
            prev_timestamp_ms: head.timestamp_ms,
        };
        let timestamp_ms = ledger_timestamp_ms(now_ms(), head.timestamp_ms);
        let built = build(&signer, &at, timestamp_ms).map_err(|error| build_error(&error))?;
        loaded
            .state
            .apply(&built.signed_event)
            .map_err(|reason| fold_error(&reason).with_detail("at_seq", at.seq))?;
        self.store(loaded.ledger)
            .append(&[NewEvent {
                seq: at.seq,
                event_id: built.event_id,
                bytes: &built.signed_event,
            }])
            .map_err(storage_error)?;
        loaded.seq_of.insert(built.event_id, at.seq);
        loaded.event_ids.push(Some(built.event_id));
        loaded.events.push(built.signed_event.clone());
        loaded.head_seq = at.seq;
        loaded.head_event = built.event_id;
        Ok(AppendedEvent {
            event_id: built.event_id,
            seq: at.seq,
            timestamp_ms,
            bytes: built.signed_event,
        })
    }

    /// Refuses to go on when a stored chain does not verify to its head.
    ///
    /// # Errors
    ///
    /// Returns code 20 carrying `valid_to_seq` and `failed_at_seq`.
    pub fn require_valid(&self, loaded: &LoadedLedger) -> Result<(), ServiceError> {
        let Some(violation) = &loaded.violation else {
            return Ok(());
        };
        Err(
            ServiceError::ledger(violation.code(), loaded.failure_message(violation))
                .with_detail("ledger_id", loaded.ledger.to_string())
                .with_detail("valid_to_seq", loaded.valid_to_seq())
                .with_detail("failed_at_seq", violation.seq)
                .with_detail(
                    "failed_event",
                    loaded
                        .failed_event
                        .map(|event| ids::event(event).as_str().to_owned()),
                ),
        )
    }

    /// The key that signs for an identity, following the `controlled_by` link
    /// of an identity-rooted ledger.
    ///
    /// # Errors
    ///
    /// Returns code 60 for a group- or world-accessible key file and code 2
    /// when neither this identity nor the identity it names holds a key.
    pub fn signing_key(&self, identity: IdentityId) -> Result<SecretKey, ServiceError> {
        self.home.identity_active_key(identity).map_err(|error| {
            if error.is_insecure_permissions() {
                return storage_error(error);
            }
            ServiceError::usage(
                "no_signing_key",
                format!("this home holds no key that may sign for {identity}"),
            )
            .with_detail("identity", identity.to_string())
        })
    }

    /// Whether this home holds the key of every controller of `state`, so no
    /// other party can append to that ledger.
    ///
    /// A ledger this wallet solely controls needs no head query before an
    /// append: nobody else can have moved it (proposal 001 section 5).
    #[must_use]
    pub fn solely_controls(&self, state: &LedgerState) -> bool {
        let controllers = state.controller_keys();
        if controllers.is_empty() {
            return false;
        }
        controllers
            .iter()
            .all(|key| self.holds_key(key).unwrap_or(false))
    }

    /// Whether some identity in this home signs under `key`.
    fn holds_key(&self, key: &iroh_base::PublicKey) -> Result<bool, ServiceError> {
        for identity in self.home.identities().map_err(storage_error)? {
            if let Ok(secret) = self.home.identity_active_key(identity)
                && &secret.public() == key
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The witnesses a ledger is pushed to and verified against: the set its
    /// own `WitnessConfig` records, plus the node-wide default of `node.json`.
    ///
    /// # Errors
    ///
    /// Returns code 10 for a malformed `node.json`.
    pub fn witnesses_of(&self, ledger: LedgerId) -> Result<Vec<EndpointId>, ServiceError> {
        let mut witnesses = match self.load(ledger) {
            Ok(loaded) => loaded.state.witnesses().to_vec(),
            Err(_) => Vec::new(),
        };
        for endpoint in self.config()?.witnesses {
            if !witnesses.contains(&endpoint) {
                witnesses.push(endpoint);
            }
        }
        Ok(witnesses)
    }

    /// Stores a run of events a peer served, keeping the bytes verbatim.
    ///
    /// The caller has already verified the run from nothing. Events the home
    /// already holds are skipped, which makes a repeated fetch idempotent.
    ///
    /// # Errors
    ///
    /// Returns code 50 when the stored copy diverges from the run, and the
    /// storage errors of the append.
    pub fn store_events(
        &self,
        ledger: LedgerId,
        events: &[Vec<u8>],
        source: Option<EndpointId>,
    ) -> Result<u64, ServiceError> {
        let store = self.store(ledger);
        let held = store
            .head()
            .map_err(storage_error)?
            .map_or(0, |head| head.seq + 1);
        for seq in 0..held.min(events.len() as u64) {
            let stored = store.read_event(seq).map_err(storage_error)?;
            if stored != events[seq as usize] {
                return Err(ServiceError::state(
                    "divergent_local_copy",
                    format!("this node holds a different event at seq {seq} of {ledger}"),
                )
                .with_detail("ledger_id", ledger.to_string())
                .with_detail("at_seq", seq));
            }
        }
        let mut batch = Vec::new();
        for (index, bytes) in events.iter().enumerate().skip(held as usize) {
            batch.push(NewEvent {
                seq: index as u64,
                event_id: event_id_of(bytes)?,
                bytes,
            });
        }
        let stored = batch.len() as u64;
        store.append(&batch).map_err(storage_error)?;
        store.note_first_seen(source).map_err(storage_error)?;
        Ok(stored)
    }

    /// Drops every event past `keep_through_seq` and rebuilds the head cache.
    ///
    /// This is what discarding a local unpushed event that lost a race means
    /// on disk (proposal 001 section 5).
    ///
    /// # Errors
    ///
    /// Returns the storage errors of removing the files and rebuilding the
    /// cache.
    pub fn truncate(&self, ledger: LedgerId, keep_through_seq: u64) -> Result<(), ServiceError> {
        let store = self.store(ledger);
        for seq in store.sequences().map_err(storage_error)? {
            if seq > keep_through_seq {
                remove(&store.event_path(seq))?;
            }
        }
        remove(&store.head_path())?;
        store.rebuild_head().map_err(storage_error)?;
        Ok(())
    }

    /// Bytes of ledger data this home holds.
    ///
    /// # Errors
    ///
    /// Returns the storage errors of walking `ledgers/`.
    pub fn storage_used(&self) -> Result<u64, ServiceError> {
        let mut used = 0;
        for ledger in self.home.ledgers().map_err(storage_error)? {
            let store = self.store(ledger);
            for seq in store.sequences().map_err(storage_error)? {
                used += fs::metadata(store.event_path(seq)).map_or(0, |meta| meta.len());
            }
        }
        Ok(used)
    }

    /// The `Appended` document for an event that just landed.
    fn appended_document(
        &self,
        ledger: LedgerId,
        appended: &AppendedEvent,
    ) -> Result<Appended, ServiceError> {
        Ok(Appended {
            ledger_id: ids::identity(ledger),
            head_seq: appended.seq,
            head_event: ids::event(appended.event_id),
            event: event_document(&appended.bytes)?,
        })
    }

    fn refuse_reused_alias(&self, alias: &str) -> Result<(), ServiceError> {
        for identity in self.home.identities().map_err(storage_error)? {
            if self
                .home
                .identity_meta(identity)
                .map_err(storage_error)?
                .alias
                == alias
            {
                return Err(ServiceError::usage(
                    "alias_in_use",
                    format!("{alias} already names {identity} in this home"),
                )
                .with_detail("alias", alias)
                .with_detail("identity", identity.to_string()));
            }
        }
        Ok(())
    }
}

/// The 404 a ledger this home does not hold answers.
#[must_use]
pub fn unknown_ledger(ledger: LedgerId) -> ServiceError {
    ServiceError::usage(
        "unknown_ledger",
        format!("this home holds no ledger {ledger}"),
    )
    .with_detail("ledger_id", ledger.to_string())
    .with_status(axum::http::StatusCode::NOT_FOUND)
}

/// The id of an encoded `SignedEvent`.
///
/// # Errors
///
/// Returns code 10 when the bytes do not decode.
pub fn event_id_of(bytes: &[u8]) -> Result<EventId, ServiceError> {
    use mabel_proto::prost::Message;
    mabel_proto::v0::SignedEvent::decode(bytes)
        .map(|signed| mabel_core::event_id(&signed.body))
        .map_err(|error| {
            ServiceError::schema(
                "malformed_event",
                format!("a served event does not decode: {error}"),
            )
        })
}

fn remove(path: &std::path::Path) -> Result<(), ServiceError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ServiceError::state(
            "storage_unavailable",
            format!("{} could not be removed: {error}", path.display()),
        )
        .with_status(axum::http::StatusCode::INTERNAL_SERVER_ERROR)),
    }
}

fn proto_kind(kind: DocumentKind) -> mabel_proto::v0::DeclaredKind {
    match kind {
        DocumentKind::Person => mabel_proto::v0::DeclaredKind::Person,
        DocumentKind::Organization => mabel_proto::v0::DeclaredKind::Organization,
        DocumentKind::Agent => mabel_proto::v0::DeclaredKind::Agent,
        DocumentKind::Service => mabel_proto::v0::DeclaredKind::Service,
    }
}

fn stored_kind(kind: DocumentKind) -> DeclaredKind {
    match kind {
        DocumentKind::Person => DeclaredKind::Person,
        DocumentKind::Organization => DeclaredKind::Organization,
        DocumentKind::Agent => DeclaredKind::Agent,
        DocumentKind::Service => DeclaredKind::Service,
    }
}
