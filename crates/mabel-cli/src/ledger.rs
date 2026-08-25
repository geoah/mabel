//! Reading a stored ledger back through the fold.
//!
//! Every command that reports on a ledger, and every command that appends to
//! one, folds the stored events first: the fold is the only thing that decides
//! what a ledger says (proposal 001 section 3.6). Nothing here trusts
//! `head.json`, which is a cache.
//!
//! The fold records an attestation by event id and not by position, so this
//! module keeps the position of every event it accepted and looks the two up
//! together when a document needs `attestation_seq` or `revocation_seq`.

use std::collections::BTreeMap;

use mabel_core::fold::{LedgerRoot, LedgerState, Reason, Violation};
use mabel_core::{EventId, LedgerId, event_id};
use mabel_node::LedgerStore;
use mabel_node::api::documents::{
    DeclaredKind, Id, Identity, PrincipalEntry, Profile, RoleName, SigningPrincipal, TrustEntry,
    Verification,
};
use mabel_proto::prost::Message;
use mabel_proto::v0::{EventBody, SignedEvent};

use crate::error::{CliError, Result};
use crate::ids;

/// One ledger as this home holds it, folded.
pub struct Loaded {
    /// The ledger.
    pub ledger: LedgerId,
    /// The fold of the valid prefix.
    pub state: LedgerState,
    /// Position of every event the fold accepted, by event id.
    pub seq_of: BTreeMap<EventId, u64>,
    /// `timestamp_ms` of the seq-0 event.
    pub created_at_ms: u64,
    /// Position of the last stored event, valid or not.
    pub head_seq: u64,
    /// Id of the last stored event, valid or not.
    pub head_event: EventId,
    /// Events on disk.
    pub event_count: u64,
    /// The first event the fold rejected, if one failed.
    pub violation: Option<Violation>,
    /// Its event id, absent when the bytes did not even decode.
    pub failed_event: Option<EventId>,
}

impl Loaded {
    /// Folds every stored event of `store`.
    ///
    /// # Errors
    ///
    /// Returns the storage errors of [`LedgerStore::read_all`], and code 2
    /// when the home holds no events for this ledger.
    pub fn open(store: &LedgerStore) -> Result<Self> {
        let ledger = store.ledger_id();
        let stored = store.read_all()?;
        if stored.is_empty() {
            return Err(CliError::usage(
                "unknown_ledger",
                format!("this home holds no ledger {}", crate::ids::shown(ledger)),
            )
            .with_detail("ledger_id", ledger.to_string()));
        }

        let mut loaded = Self {
            ledger,
            state: LedgerState::default(),
            seq_of: BTreeMap::new(),
            created_at_ms: 0,
            head_seq: 0,
            head_event: EventId::from_bytes([0u8; 32]),
            event_count: stored.len() as u64,
            violation: None,
            failed_event: None,
        };
        for event in &stored {
            let id = decoded_id(&event.bytes);
            if let Some(id) = id {
                loaded.head_seq = event.seq;
                loaded.head_event = id;
            }
            if loaded.violation.is_some() {
                continue;
            }
            match loaded.state.apply(&event.bytes) {
                Ok(()) => {
                    if let Some(id) = id {
                        loaded.seq_of.insert(id, event.seq);
                    }
                    if event.seq == 0 {
                        loaded.created_at_ms = timestamp_of(&event.bytes);
                    }
                }
                Err(reason) => {
                    loaded.violation = Some(Violation {
                        seq: event.seq,
                        reason,
                    });
                    loaded.failed_event = id;
                }
            }
        }
        Ok(loaded)
    }

    /// The last position the fold accepted.
    #[must_use]
    pub fn valid_to_seq(&self) -> u64 {
        self.state.head().map_or(0, |head| head.seq)
    }

    /// The sentence a partial chain reports: how far it verified and where it
    /// broke (proposal 001 section 3.6).
    #[must_use]
    pub fn failure_message(&self, violation: &Violation) -> String {
        format!(
            "valid to seq {}, failed at seq {}: {}",
            self.valid_to_seq(),
            violation.seq,
            violation.reason
        )
    }

    /// Refuses to go on when the stored chain does not verify to its head.
    ///
    /// Partial validity is a failure, not a result: nothing is appended to a
    /// ledger whose stored prefix the fold already rejected.
    ///
    /// # Errors
    ///
    /// Returns code 20, `Ledger error:`, carrying `valid_to_seq` and
    /// `failed_at_seq`.
    pub fn require_valid(&self) -> Result<()> {
        let Some(violation) = &self.violation else {
            return Ok(());
        };
        Err(
            CliError::ledger(violation.code(), self.failure_message(violation))
                .with_detail("ledger_id", self.ledger.to_string())
                .with_detail("valid_to_seq", self.valid_to_seq())
                .with_detail("failed_at_seq", violation.seq)
                .with_detail("failed_event", self.failed_event.map(ids::event)),
        )
    }

    /// The envelope for an event the fold refused to apply.
    ///
    /// A duplicate attestation is spelled as `contracts/cli/errors.json`
    /// spells it, since the fold names the standing attestation but not its
    /// position.
    #[must_use]
    pub fn rejection(&self, reason: &Reason, at_seq: u64) -> CliError {
        if let Reason::DuplicateAttestation {
            subject,
            attestation,
        } = reason
        {
            let at = self.seq_of.get(attestation).copied().unwrap_or_default();
            return CliError::policy(
                "duplicate_unrevoked_attestation",
                format!(
                    "an unrevoked attestation for {} already exists at seq {at}",
                    crate::ids::shown(subject)
                ),
            )
            .with_detail("ledger_id", self.ledger.to_string())
            .with_detail("subject", subject.to_string())
            .with_detail("attestation_event", attestation.to_string())
            .with_detail("at_seq", at);
        }
        CliError::from(reason)
            .with_detail("ledger_id", self.ledger.to_string())
            .with_detail("at_seq", at_seq)
    }

    /// What the inception declared, `person` unless it says otherwise.
    #[must_use]
    pub fn declared_kind(&self) -> DeclaredKind {
        self.state
            .declared_kind()
            .and_then(|kind| DeclaredKind::parse(mabel_core::declared_kind_name(kind)))
            .unwrap_or(DeclaredKind::Person)
    }

    /// The identities the latest `WitnessSet` names, which are the identities
    /// that may keep this ledger (proposal 006 section 1).
    #[must_use]
    pub fn witnesses(&self) -> Vec<Id> {
        self.state
            .witness_identities()
            .iter()
            .copied()
            .map(ids::identity)
            .collect()
    }

    /// The machines the latest `EndpointAdvertisement` names, which are where
    /// this identity answers (proposal 006 section 2).
    #[must_use]
    pub fn endpoints(&self) -> Vec<Id> {
        self.state.endpoints().iter().map(ids::key).collect()
    }

    /// The endpoints the latest `WitnessConfig` names, payload tag 11.
    ///
    /// Nothing writes tag 11 any more and every chain written before proposal
    /// 006 may hold one, so the document reports both lists rather than
    /// merging them: they come from different payloads and mean different
    /// things.
    #[must_use]
    pub fn witness_endpoints(&self) -> Vec<Id> {
        self.state
            .witness_endpoints()
            .iter()
            .map(ids::key)
            .collect()
    }

    /// Every attestation this ledger issued, revoked ones included, by
    /// ascending position.
    #[must_use]
    pub fn trust(&self) -> Vec<TrustEntry> {
        let mut entries: Vec<TrustEntry> = self
            .state
            .trust()
            .iter()
            .map(|(event, attestation)| TrustEntry {
                attestation_event: ids::event(*event),
                attestation_seq: self.seq_of.get(event).copied().unwrap_or_default(),
                subject: ids::identity(attestation.subject),
                revoked: attestation.is_revoked(),
                revocation_event: attestation.revoked_by.map(ids::event),
                revocation_seq: attestation
                    .revoked_by
                    .and_then(|event| self.seq_of.get(&event).copied()),
            })
            .collect();
        entries.sort_by_key(|entry| entry.attestation_seq);
        entries
    }

    /// The identity document `contracts/README.md` shares between the HTTP
    /// routes and `mabel identity list`.
    ///
    /// `active_key` and `reserve_commit` belong to a raw root; an
    /// identity-rooted ledger holds no key of its own and omits them.
    #[must_use]
    pub fn identity_document(&self, alias: String) -> Identity {
        let (active_key, reserve_commit) = match self.state.root() {
            Some(LedgerRoot::Raw {
                active_key,
                reserve_commit,
            }) => (
                Some(ids::key(&active_key)),
                Some(ids::bytes(&reserve_commit)),
            ),
            _ => (None, None),
        };
        let profile = self.profile();
        let verification = match profile
            .as_ref()
            .and_then(|profile| profile.hostname.as_deref())
        {
            Some(hostname) => Verification::unchecked(hostname),
            None => Verification::unclaimed(),
        };
        Identity {
            identity_id: ids::identity(self.ledger),
            declared_kind: self.declared_kind(),
            alias,
            created_at_ms: self.created_at_ms,
            head_seq: self.head_seq,
            head_event: ids::event(self.head_event),
            event_count: self.event_count,
            witnesses: self.witnesses(),
            endpoints: self.endpoints(),
            witness_endpoints: self.witness_endpoints(),
            trust: self.trust(),
            profile,
            verification,
            contact: None,
            active_key,
            reserve_commit,
            principals: self.principal_entries(),
            open_invitation_count: self.open_invitation_count(),
        }
    }

    /// The profile the latest `ProfileUpdate` left, `None` on a ledger that
    /// carries none (proposal 003 section 1).
    #[must_use]
    pub fn profile(&self) -> Option<Profile> {
        let profile = self.state.profile()?;
        Some(Profile {
            display_name: profile.display_name.clone(),
            hostname: profile.hostname.clone(),
            email: profile.email.clone(),
            signing_principal: SigningPrincipal {
                identity: ids::identity(profile.signing_principal.identity),
                key: ids::key(&profile.signing_principal.key),
            },
            event: ids::event(profile.event),
            seq: profile.seq,
        })
    }

    /// The `principals` array of the identity document, root first by the
    /// fold's ordering, skipping any principal without a recorded role.
    fn principal_entries(&self) -> Vec<PrincipalEntry> {
        let root = self.state.root_identity();
        self.state
            .principals()
            .iter()
            .filter_map(|(identity, principal)| {
                let role = match principal.role {
                    mabel_core::proto::Role::Member => RoleName::Member,
                    mabel_core::proto::Role::Controller => RoleName::Controller,
                    mabel_core::proto::Role::Unspecified => return None,
                };
                Some(PrincipalEntry {
                    identity: ids::identity(*identity),
                    active_key: ids::key(&principal.active_key),
                    role,
                    is_root: Some(*identity) == root,
                })
            })
            .collect()
    }

    /// Invitations still open, the count the identity document carries.
    fn open_invitation_count(&self) -> u64 {
        self.state
            .invitations()
            .values()
            .filter(|invitation| invitation.status == mabel_core::fold::InvitationStatus::Open)
            .count() as u64
    }
}

/// The event id of stored bytes, or `None` if they do not decode.
fn decoded_id(bytes: &[u8]) -> Option<EventId> {
    SignedEvent::decode(bytes)
        .ok()
        .map(|signed| event_id(&signed.body))
}

/// The `timestamp_ms` of stored bytes the fold has already accepted.
fn timestamp_of(bytes: &[u8]) -> u64 {
    SignedEvent::decode(bytes)
        .ok()
        .and_then(|signed| EventBody::decode(&signed.body[..]).ok())
        .map_or(0, |body| body.timestamp_ms)
}
