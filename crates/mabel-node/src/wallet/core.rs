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

use data_encoding::BASE64;
use iroh_base::{EndpointId, SecretKey};
use mabel_core::artifacts::{AcceptanceFile, IdentityDescriptor, InvitationBundle};
use mabel_core::fold::{InvitationStatus, LedgerState};
use mabel_core::sign::{
    BuildError, BuiltEvent, Position, Root, build_acceptance, build_inception,
    build_membership_acceptance, build_membership_invitation, build_membership_removal,
    build_trust_attestation, build_trust_revocation, build_witness_config, ledger_timestamp_ms,
};
use mabel_core::{EventId, IdentityId, LedgerId, NONCE_BYTES};
use mabel_proto::prost::Message;
use mabel_proto::v0 as pb;
use mabel_proto::v0::event_body::Payload;

use crate::api::documents::{
    Accepted, Admitted, Appended, CreatedIdentity, DeclaredKind as DocumentKind, Identity, Invited,
    LedgerPage, MembershipView, PrincipalEntry, Removed, Revoked, RoleName, RootName,
};
use crate::api::error::ServiceError;
use crate::api::service::EventPageRequest;
use crate::config::NodeConfig;
use crate::home::{DeclaredKind, IdentityMeta, NodeHome};
use crate::ledger::{LedgerMeta, LedgerStore, NewEvent};
use crate::now_ms;
use crate::wallet::error::{artifact_error, build_error, fold_error, storage_error};
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

    /// Mints an identity: a seq-0 event under a raw root, keyed by a fresh
    /// active key and a reserve commitment, or under an identity root founded
    /// by `founder` (proposal 002 section 2).
    ///
    /// With a founder the new ledger holds no key of its own: the founder's
    /// key signs the inception, and `controlled_by` records which local
    /// identity signs for it later.
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
        founder: Option<IdentityId>,
    ) -> Result<CreatedIdentity, ServiceError> {
        self.refuse_reused_alias(alias)?;
        let mut nonce = [0u8; NONCE_BYTES];
        getrandom::fill(&mut nonce).map_err(|error| {
            ServiceError::state("no_randomness", error.to_string())
                .with_status(axum::http::StatusCode::INTERNAL_SERVER_ERROR)
        })?;
        let created_at_ms = now_ms();

        let (built, keys) = match founder {
            Some(founder) => {
                let signer = self.signing_key(founder)?;
                let inception = self
                    .store(founder)
                    .read_event(0)
                    .map_err(|_| unknown_ledger(founder))?;
                let built = build_inception(
                    &signer,
                    proto_kind(declared_kind),
                    Root::Identity {
                        founder,
                        founder_inception: &inception,
                    },
                    nonce,
                    created_at_ms,
                )
                .map_err(|error| build_error(&error))?;
                (built, None)
            }
            None => {
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
                (built, Some((active, reserve)))
            }
        };

        // The fold decides whether these bytes are a ledger before they land.
        let mut state = LedgerState::default();
        state
            .apply(&built.signed_event)
            .map_err(|reason| fold_error(&reason))?;
        let identity = IdentityId::from(built.event_id);

        if let Some((active, reserve)) = &keys {
            self.home
                .write_identity_keys(identity, active, reserve)
                .map_err(storage_error)?;
        }
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
                    controlled_by: founder,
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

    /// The principals and invitations of one stored ledger.
    ///
    /// # Errors
    ///
    /// As [`WalletCore::load`].
    pub fn memberships(&self, ledger: LedgerId) -> Result<MembershipView, ServiceError> {
        Ok(self.load(ledger)?.membership_document())
    }

    /// Appends a `MembershipInvitation` naming the identity `descriptor`
    /// describes, and returns the bundle the invitee needs to accept it.
    ///
    /// The invitation embeds the inception the descriptor carries, byte for
    /// byte, which is what proves the invitee's id and key belong together
    /// (proposal 002 section 8). The bundle is built after the event lands, so
    /// it holds the ledger the invitee will fold.
    ///
    /// # Errors
    ///
    /// Returns code 10 for a descriptor that is not one, code 20 for an
    /// invitee that holds no key of its own or a rule the fold refuses, and
    /// the errors of [`WalletCore::append`].
    pub fn invite(
        &self,
        ledger: LedgerId,
        by: IdentityId,
        role: RoleName,
        descriptor: &[u8],
    ) -> Result<Invited, ServiceError> {
        let descriptor = IdentityDescriptor::read(descriptor)
            .map_err(|error| artifact_error("IdentityDescriptor", &error))?;
        let invitee = descriptor.identity();
        let invitee_key = descriptor.active_key().ok_or_else(|| {
            ServiceError::policy(
                "invitee_holds_no_key",
                format!(
                    "{invitee} is an identity-rooted ledger and holds no key of its own, \
                     so it cannot be invited"
                ),
            )
            .with_detail("invitee", invitee.to_string())
        })?;

        let mut loaded = self.load(ledger)?;
        let appended = self.append(by, &mut loaded, |signer, at, timestamp_ms| {
            build_membership_invitation(
                signer,
                at,
                invitee,
                &invitee_key,
                proto_role(role),
                descriptor.inception(),
                timestamp_ms,
            )
        })?;

        let bundle = InvitationBundle::new(loaded.events.clone())
            .map_err(|error| artifact_error("InvitationBundle", &error))?;
        Ok(Invited {
            ledger_id: ids::identity(ledger),
            by: ids::identity(by),
            invitee: ids::identity(invitee),
            invitee_key: ids::key(&invitee_key),
            role,
            invitation_event: ids::event(appended.event_id),
            invitation_seq: appended.seq,
            timestamp_ms: appended.timestamp_ms,
            head_seq: appended.seq,
            head_event: ids::event(appended.event_id),
            event: event_document(&appended.bytes)?,
            invitation_bundle_base64: BASE64.encode(&bundle.write()),
            event_count: bundle.events().len() as u64,
        })
    }

    /// Signs the invitee's acceptance of the invitation `bundle` carries, and
    /// returns the accept surface it was signed under.
    ///
    /// Nothing is appended here: the acceptance is a detached file a
    /// controller of the invited ledger admits later. The bundle is folded
    /// from its inception before anything is signed, so the surface is the
    /// fold's answer and not the file's claim (proposal 002 section 4).
    ///
    /// # Errors
    ///
    /// Returns code 10 for a bundle that is not one, code 2 when the
    /// invitation names another identity, code 20 when this home signs under
    /// another key, and code 60 for an insecure key file.
    pub fn accept_invitation(
        &self,
        identity: IdentityId,
        bundle: &[u8],
    ) -> Result<Accepted, ServiceError> {
        let bundle = InvitationBundle::read(bundle)
            .map_err(|error| artifact_error("InvitationBundle", &error))?;
        let summary = bundle
            .summary()
            .map_err(|error| artifact_error("InvitationBundle", &error))?;

        if summary.invitee != identity {
            return Err(ServiceError::usage(
                "not_the_invitee",
                format!(
                    "this invitation invites {}, not {identity}",
                    summary.invitee
                ),
            )
            .with_detail("ledger_id", summary.ledger.to_string())
            .with_detail("invitee", summary.invitee.to_string()));
        }
        let key = self.signing_key(identity)?;
        if key.public() != summary.invitee_key {
            return Err(ServiceError::policy(
                "acceptance_invitee_key_mismatch",
                format!(
                    "the invitation records key {} for {identity}, and this home signs with {}",
                    summary.invitee_key,
                    key.public()
                ),
            )
            .with_detail("ledger_id", summary.ledger.to_string()));
        }

        let signed = build_acceptance(&key, summary.ledger, summary.invitation_event, identity);
        let file = AcceptanceFile::new(&signed)
            .map_err(|error| artifact_error("AcceptanceFile", &error))?;

        let root_identity = match summary.root {
            mabel_core::fold::LedgerRoot::Raw { .. } => summary.ledger,
            mabel_core::fold::LedgerRoot::Identity { founder, .. } => founder,
        };
        let controllers = summary
            .controllers
            .iter()
            .map(|principal| PrincipalEntry {
                identity: ids::identity(principal.identity),
                active_key: ids::key(&principal.key),
                role: RoleName::Controller,
                is_root: principal.identity == root_identity,
            })
            .collect();
        let role = role_name(summary.role)?;
        let controller_on_raw_root = summary.controller_on_raw_root();
        Ok(Accepted {
            ledger_id: ids::identity(summary.ledger),
            declared_kind: crate::witness::events::declared_kind(summary.declared_kind),
            root: root_name(summary.root),
            controllers,
            invitation_event: ids::event(summary.invitation_event),
            invitee: ids::identity(summary.invitee),
            invitee_key: ids::key(&summary.invitee_key),
            role,
            controller_on_raw_root,
            warning: controller_on_raw_root.then(|| raw_root_warning(summary.ledger)),
            acceptance_base64: BASE64.encode(&file.write()),
        })
    }

    /// Appends a `MembershipAcceptance` carrying `acceptance`, which admits
    /// the principal the invitation it names records.
    ///
    /// # Errors
    ///
    /// Returns code 10 for a file that is not an acceptance, code 50 with
    /// reason `acceptance_already_used` when this ledger already admitted it,
    /// and the errors of [`WalletCore::append`].
    pub fn admit_acceptance(
        &self,
        ledger: LedgerId,
        by: IdentityId,
        acceptance: &[u8],
    ) -> Result<Admitted, ServiceError> {
        let file = AcceptanceFile::read(acceptance)
            .map_err(|error| artifact_error("AcceptanceFile", &error))?;
        let mut loaded = self.load(ledger)?;
        self.refuse_replay(&loaded, file.invitation_event())?;

        // The invitation is what the acceptance admits (proposal 002
        // section 4), so the role and key reported below are read from it,
        // before the append marks it accepted.
        let invitation = loaded.state.invitation(&file.invitation_event()).copied();
        let detached = file.detached();
        let appended = self.append(by, &mut loaded, |signer, at, timestamp_ms| {
            build_membership_acceptance(signer, at, &detached, timestamp_ms)
        })?;
        let Some(invitation) = invitation else {
            // The fold rejects an acceptance naming no invitation, so this is
            // unreachable.
            return Err(ServiceError::ledger(
                "invitation_not_folded",
                format!(
                    "invitation {} is not in ledger {ledger}",
                    file.invitation_event()
                ),
            ));
        };

        Ok(Admitted {
            ledger_id: ids::identity(ledger),
            by: ids::identity(by),
            invitee: ids::identity(invitation.invitee),
            invitee_key: ids::key(&invitation.invitee_key),
            role: role_name(invitation.role)?,
            invitation_event: ids::event(file.invitation_event()),
            acceptance_event: ids::event(appended.event_id),
            acceptance_seq: appended.seq,
            timestamp_ms: appended.timestamp_ms,
            head_seq: appended.seq,
            head_event: ids::event(appended.event_id),
            event: event_document(&appended.bytes)?,
        })
    }

    /// Appends a `MembershipRemoval` naming `target`.
    ///
    /// One removal cancels an open invitation and takes away a principal,
    /// whichever exist. The raw root and the last controller are the fold's to
    /// refuse.
    ///
    /// # Errors
    ///
    /// As [`WalletCore::append`].
    pub fn remove_membership(
        &self,
        ledger: LedgerId,
        by: IdentityId,
        target: IdentityId,
    ) -> Result<Removed, ServiceError> {
        let mut loaded = self.load(ledger)?;
        let principal_removed = loaded.state.principal(&target).is_some();
        let cancelled = open_invitation(&loaded.state, target);
        let appended = self.append(by, &mut loaded, |signer, at, timestamp_ms| {
            build_membership_removal(signer, at, target, timestamp_ms)
        })?;
        Ok(Removed {
            ledger_id: ids::identity(ledger),
            by: ids::identity(by),
            target: ids::identity(target),
            principal_removed,
            invitation_cancelled: cancelled.map(ids::event),
            removal_event: ids::event(appended.event_id),
            removal_seq: appended.seq,
            timestamp_ms: appended.timestamp_ms,
            head_seq: appended.seq,
            head_event: ids::event(appended.event_id),
            event: event_document(&appended.bytes)?,
        })
    }

    /// Refuses an acceptance this ledger already admitted (pitfall 4).
    ///
    /// The fold calls that state `invitation_not_open`, which is true but says
    /// nothing about the file the caller passed, so this is reported as the
    /// replay of a single-use artifact instead: code 50, `Replay error:`
    /// (`contracts/cli/errors.json`).
    fn refuse_replay(
        &self,
        loaded: &LoadedLedger,
        invitation: EventId,
    ) -> Result<(), ServiceError> {
        let Some(held) = loaded.state.invitation(&invitation) else {
            return Ok(());
        };
        if held.status != InvitationStatus::Accepted {
            return Ok(());
        }
        let at_seq = self
            .admitted_at(loaded.ledger, invitation)?
            .unwrap_or_default();
        Err(ServiceError::replay(
            "acceptance_already_used",
            format!(
                "this acceptance was already admitted at seq {at_seq} of {}",
                loaded.ledger
            ),
        )
        .with_detail("ledger_id", loaded.ledger.to_string())
        .with_detail("invitation_event", invitation.to_string())
        .with_detail("at_seq", at_seq))
    }

    /// The position of the acceptance that consumed `invitation`.
    ///
    /// The fold records that an invitation was accepted but not which event
    /// accepted it, so the stored events are scanned for the acceptance whose
    /// blob names it.
    fn admitted_at(
        &self,
        ledger: LedgerId,
        invitation: EventId,
    ) -> Result<Option<u64>, ServiceError> {
        for stored in self.store(ledger).read_all().map_err(storage_error)? {
            let Some(Payload::MembershipAcceptance(acceptance)) = payload_of(&stored.bytes) else {
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

/// The sentence the accept surface carries when accepting means signing as
/// the ledger's own identity (proposal 002 section 4).
///
/// `mabel membership accept` prints this same sentence, so a person reads one
/// wording on both surfaces.
fn raw_root_warning(ledger: LedgerId) -> String {
    format!(
        "accepting a controller role on a raw-rooted ledger means signing as {ledger}: \
         every event you append to it is that identity's own event"
    )
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

/// The payload of stored event bytes the fold has already accepted.
fn payload_of(bytes: &[u8]) -> Option<Payload> {
    pb::SignedEvent::decode(bytes)
        .ok()
        .and_then(|signed| pb::EventBody::decode(&signed.body[..]).ok())
        .and_then(|body| body.payload)
}

/// Where a folded root came from, as every document names it.
const fn root_name(root: mabel_core::fold::LedgerRoot) -> RootName {
    match root {
        mabel_core::fold::LedgerRoot::Raw { .. } => RootName::Raw,
        mabel_core::fold::LedgerRoot::Identity { .. } => RootName::Identity,
    }
}

/// The name of a role the fold recorded.
///
/// The fold never records `ROLE_UNSPECIFIED`, which the field table rejects,
/// so this cannot fail on a stored ledger.
fn role_name(role: pb::Role) -> Result<RoleName, ServiceError> {
    match role {
        pb::Role::Member => Ok(RoleName::Member),
        pb::Role::Controller => Ok(RoleName::Controller),
        pb::Role::Unspecified => Err(ServiceError::schema(
            "unspecified_role",
            "a membership event carries no recognised role",
        )),
    }
}

const fn proto_role(role: RoleName) -> pb::Role {
    match role {
        RoleName::Member => pb::Role::Member,
        RoleName::Controller => pb::Role::Controller,
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
