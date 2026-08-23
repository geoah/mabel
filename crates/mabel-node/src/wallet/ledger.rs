//! One ledger, folded, wherever its events came from.
//!
//! The same type covers a ledger this home holds and a candidate a peer
//! served: both are a run of encoded `SignedEvent` bytes, and the fold is the
//! only thing that decides what they say (proposal 001 section 3.6). Nothing
//! here trusts `head.json`, which is a cache, and nothing trusts a peer.
//!
//! The fold records an attestation by event id and not by position, so this
//! module keeps the position of every event it accepted and looks the two up
//! together when a document needs `attestation_seq`.

use std::collections::BTreeMap;

use mabel_core::fold::{InvitationStatus, LedgerRoot, LedgerState, Reason, Violation};
use mabel_core::{EventId, LedgerId, event_id};
use mabel_proto::prost::Message;
use mabel_proto::v0::{EventBody, Role, SignedEvent};

use crate::api::documents::{
    DeclaredKind, Id, Identity, InvitationEntry, MembershipView, PrincipalEntry, RoleName,
    RootName, StatusName, TrustEntry,
};
use crate::api::error::ServiceError;
use crate::ledger::LedgerStore;
use crate::wallet::ids;

/// A run of events and the state they fold to.
#[derive(Debug, Clone)]
pub struct LoadedLedger {
    /// The ledger these events claim.
    pub ledger: LedgerId,
    /// The events, as the signer or the peer produced them.
    pub events: Vec<Vec<u8>>,
    /// The fold of the valid prefix.
    pub state: LedgerState,
    /// Position of every event the fold accepted, by event id.
    pub seq_of: BTreeMap<EventId, u64>,
    /// Id of every event in order, whether the fold accepted it or not.
    pub event_ids: Vec<Option<EventId>>,
    /// `timestamp_ms` of the seq-0 event.
    pub created_at_ms: u64,
    /// Position of the last event, valid or not.
    pub head_seq: u64,
    /// Id of the last event, valid or not.
    pub head_event: EventId,
    /// The first event the fold rejected, if one failed.
    pub violation: Option<Violation>,
    /// Its event id, absent when the bytes did not even decode.
    pub failed_event: Option<EventId>,
}

impl LoadedLedger {
    /// Folds a run of events that claims to be `ledger`.
    #[must_use]
    pub fn fold(ledger: LedgerId, events: Vec<Vec<u8>>) -> Self {
        let mut loaded = Self {
            ledger,
            events,
            state: LedgerState::default(),
            seq_of: BTreeMap::new(),
            event_ids: Vec::new(),
            created_at_ms: 0,
            head_seq: 0,
            head_event: EventId::from_bytes([0u8; 32]),
            violation: None,
            failed_event: None,
        };
        for (index, bytes) in loaded.events.clone().into_iter().enumerate() {
            let seq = index as u64;
            let id = decoded_id(&bytes);
            loaded.event_ids.push(id);
            if let Some(id) = id {
                loaded.head_seq = seq;
                loaded.head_event = id;
            }
            if loaded.violation.is_some() {
                continue;
            }
            match loaded.state.apply(&bytes) {
                Ok(()) => {
                    if let Some(id) = id {
                        loaded.seq_of.insert(id, seq);
                    }
                    if seq == 0 {
                        loaded.created_at_ms = timestamp_of(&bytes);
                    }
                }
                Err(reason) => {
                    loaded.violation = Some(Violation { seq, reason });
                    loaded.failed_event = id;
                }
            }
        }
        loaded
    }

    /// Folds every event `store` holds.
    ///
    /// # Errors
    ///
    /// Returns the storage errors of reading the ledger directory.
    pub fn open(store: &LedgerStore) -> Result<Self, ServiceError> {
        let stored = store
            .read_all()
            .map_err(crate::wallet::error::storage_error)?;
        Ok(Self::fold(
            store.ledger_id(),
            stored.into_iter().map(|event| event.bytes).collect(),
        ))
    }

    /// Whether the run holds no events at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Events in the run.
    #[must_use]
    pub fn event_count(&self) -> u64 {
        self.events.len() as u64
    }

    /// The last position the fold accepted.
    #[must_use]
    pub fn valid_to_seq(&self) -> u64 {
        self.state.head().map_or(0, |head| head.seq)
    }

    /// The envelope for an event this fold refused to apply.
    ///
    /// A duplicate attestation carries the position of the attestation still
    /// standing, which the fold knows by event id alone, so both surfaces
    /// answer the one sentence `contracts/http/wallet-post-trust.json` and
    /// `contracts/cli/errors.json` pin. Every other reason carries the
    /// position the refused event would have taken.
    #[must_use]
    pub fn rejection(&self, reason: &Reason, at_seq: u64) -> ServiceError {
        if let Reason::DuplicateAttestation { attestation, .. } = reason {
            return crate::wallet::error::fold_error_at(
                reason,
                self.seq_of.get(attestation).copied(),
            )
            .with_detail("ledger_id", self.ledger.to_string());
        }
        crate::wallet::error::fold_error(reason).with_detail("at_seq", at_seq)
    }

    /// The sentence a partial chain reports (proposal 001 section 3.6).
    #[must_use]
    pub fn failure_message(&self, violation: &Violation) -> String {
        format!(
            "valid to seq {}, failed at seq {}: {}",
            self.valid_to_seq(),
            violation.seq,
            violation.reason
        )
    }

    /// What the inception declared, `person` unless it says otherwise.
    #[must_use]
    pub fn declared_kind(&self) -> DeclaredKind {
        self.state
            .declared_kind()
            .and_then(|kind| DeclaredKind::parse(mabel_core::declared_kind_name(kind)))
            .unwrap_or(DeclaredKind::Person)
    }

    /// The witness set of the latest `WitnessConfig`.
    #[must_use]
    pub fn witnesses(&self) -> Vec<Id> {
        self.state.witnesses().iter().map(ids::key).collect()
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

    /// Where this ledger's signing authority came from, `raw` on a ledger the
    /// fold has not seeded yet.
    #[must_use]
    pub fn root(&self) -> RootName {
        match self.state.root() {
            Some(LedgerRoot::Identity { .. }) => RootName::Identity,
            _ => RootName::Raw,
        }
    }

    /// Every principal the ledger records, by ascending identity id
    /// (proposal 002 section 1).
    ///
    /// A principal whose role the fold never records, `ROLE_UNSPECIFIED`, is
    /// skipped rather than given a name it does not have.
    #[must_use]
    pub fn principals(&self) -> Vec<PrincipalEntry> {
        let root = self.state.root_identity();
        self.state
            .principals()
            .iter()
            .filter_map(|(identity, principal)| {
                Some(PrincipalEntry {
                    identity: ids::identity(*identity),
                    active_key: ids::key(&principal.active_key),
                    role: role_name(principal.role)?,
                    is_root: Some(*identity) == root,
                })
            })
            .collect()
    }

    /// Every invitation the ledger issued, by ascending position, accepted and
    /// cancelled ones included.
    #[must_use]
    pub fn invitations(&self) -> Vec<InvitationEntry> {
        let mut entries: Vec<InvitationEntry> = self
            .state
            .invitations()
            .iter()
            .filter_map(|(event, invitation)| {
                Some(InvitationEntry {
                    invitation_event: ids::event(*event),
                    invitation_seq: self.seq_of.get(event).copied().unwrap_or_default(),
                    invitee: ids::identity(invitation.invitee),
                    invitee_key: ids::key(&invitation.invitee_key),
                    role: role_name(invitation.role)?,
                    status: status_name(invitation.status),
                })
            })
            .collect();
        entries.sort_by_key(|entry| entry.invitation_seq);
        entries
    }

    /// Invitations still open, which is the count the identity document
    /// carries.
    #[must_use]
    pub fn open_invitation_count(&self) -> u64 {
        self.state
            .invitations()
            .values()
            .filter(|invitation| invitation.status == InvitationStatus::Open)
            .count() as u64
    }

    /// The membership document of `GET
    /// /api/identities/{identity_id}/memberships`.
    #[must_use]
    pub fn membership_document(&self) -> MembershipView {
        MembershipView {
            ledger_id: ids::identity(self.ledger),
            declared_kind: self.declared_kind(),
            root: self.root(),
            head_seq: self.head_seq,
            head_event: ids::event(self.head_event),
            principals: self.principals(),
            invitations: self.invitations(),
        }
    }

    /// The identity document `contracts/README.md` shares between the HTTP
    /// routes and `mabel identity list`.
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
        Identity {
            identity_id: ids::identity(self.ledger),
            declared_kind: self.declared_kind(),
            alias,
            created_at_ms: self.created_at_ms,
            head_seq: self.head_seq,
            head_event: ids::event(self.head_event),
            event_count: self.event_count(),
            witnesses: self.witnesses(),
            trust: self.trust(),
            principals: self.principals(),
            open_invitation_count: self.open_invitation_count(),
            active_key,
            reserve_commit,
        }
    }
}

/// The name of a role the fold recorded.
///
/// The fold never records `ROLE_UNSPECIFIED`, which the field table rejects,
/// so this is `None` only on a value this build does not know.
fn role_name(role: Role) -> Option<RoleName> {
    match role {
        Role::Member => Some(RoleName::Member),
        Role::Controller => Some(RoleName::Controller),
        Role::Unspecified => None,
    }
}

/// The name of an invitation status the fold recorded.
const fn status_name(status: InvitationStatus) -> StatusName {
    match status {
        InvitationStatus::Open => StatusName::Open,
        InvitationStatus::Accepted => StatusName::Accepted,
        InvitationStatus::Cancelled => StatusName::Cancelled,
    }
}

/// The event id of stored bytes, or `None` if they do not decode.
fn decoded_id(bytes: &[u8]) -> Option<EventId> {
    SignedEvent::decode(bytes)
        .ok()
        .map(|signed| event_id(&signed.body))
}

/// The `timestamp_ms` of bytes the fold has already accepted.
fn timestamp_of(bytes: &[u8]) -> u64 {
    SignedEvent::decode(bytes)
        .ok()
        .and_then(|signed| EventBody::decode(&signed.body[..]).ok())
        .map_or(0, |body| body.timestamp_ms)
}
