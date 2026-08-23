//! The membership routes over two real wallet homes (ticket 021).
//!
//! One admission crosses two wallets: Alice's home invites, Bob's home signs
//! the acceptance, Alice's home admits it. Every step goes through
//! [`WalletService`], the same trait the axum handlers call, so what the
//! fixtures freeze and what the node does are the same shapes.
//!
//! Nothing here touches the network. Each wallet binds an endpoint with relays
//! disabled because the service owns one, and no ledger in this file records a
//! witness, so the append discipline of proposal 001 section 5 has nobody to
//! ask.

use std::sync::Arc;

use data_encoding::BASE64;
use mabel_core::artifacts::IdentityDescriptor;
use mabel_node::api::documents::{DeclaredKind, Id, RoleName, RootName, StatusName};
use mabel_node::api::service::{
    AcceptInvitation, AddTrust, AdmitAcceptance, CreateIdentity, Invite, RemoveMembership,
    WalletService,
};
use mabel_node::api::{DEFAULT_HTTP_BIND, ServiceError};
use mabel_node::wallet::{WalletApiService, WalletCore, WalletSync};
use mabel_node::{HomeOptions, NodeConfig, NodeHome, NodeRole, RelayMode};
use tempfile::TempDir;

/// One wallet home and the HTTP service over it.
struct Wallet {
    _dir: TempDir,
    core: Arc<WalletCore>,
    service: WalletApiService,
}

impl Wallet {
    /// A fresh home with relays disabled.
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temp directory");
        let config = NodeConfig {
            role: NodeRole::Wallet,
            relay: RelayMode::Disabled,
            ..NodeConfig::default()
        };
        let home = NodeHome::create(dir.path(), &config, HomeOptions::default())
            .expect("the home is created");
        let core = Arc::new(WalletCore::new(home));
        let secret = core.home().node_key().expect("the node key reads");
        let endpoint = mabel_node::bind_endpoint(RelayMode::Disabled, secret, None, &[])
            .await
            .expect("the endpoint binds");
        let service = WalletApiService::new(
            Arc::clone(&core),
            WalletSync::new(endpoint),
            DEFAULT_HTTP_BIND,
            RelayMode::Disabled,
        );
        Self {
            _dir: dir,
            core,
            service,
        }
    }

    /// Mints a raw-rooted identity and returns the id documents spell.
    async fn identity(&self, alias: &str) -> Id {
        self.service
            .create_identity(CreateIdentity {
                alias: alias.to_owned(),
                declared_kind: DeclaredKind::Person,
                founder: None,
            })
            .await
            .expect("the identity is created")
            .identity
            .identity_id
    }

    /// The `IdentityDescriptor` bytes an inviter needs, as `mabel identity
    /// export` writes them.
    fn descriptor(&self, identity: &Id) -> Vec<u8> {
        let parsed = identity.as_str().parse().expect("a rendered id parses");
        let inception = self
            .core
            .store(parsed)
            .read_event(0)
            .expect("the inception is stored");
        IdentityDescriptor::new(&inception, &[])
            .expect("the inception folds")
            .write()
    }
}

fn decode(value: &str) -> Vec<u8> {
    BASE64.decode(value.as_bytes()).expect("a base64 artifact")
}

#[tokio::test]
async fn two_wallets_carry_one_admission_from_invitation_to_removal() {
    let inviter = Wallet::new().await;
    let invitee = Wallet::new().await;
    let alice = inviter.identity("alice").await;
    let bob = invitee.identity("bob").await;

    // Alice invites Bob as a controller of her own raw-rooted ledger, which
    // is what delegation is under proposal 002 section 4.
    let invited = inviter
        .service
        .invite(Invite {
            ledger_id: alice.clone(),
            by: alice.clone(),
            role: RoleName::Controller,
            invitee_descriptor: invitee.descriptor(&bob),
        })
        .await
        .expect("the invitation lands");
    assert_eq!(invited.invitee, bob);
    assert_eq!(invited.role, RoleName::Controller);
    assert_eq!(invited.invitation_seq, 1);
    assert_eq!(invited.event.payload_kind, "membership_invitation");
    assert_eq!(invited.event_count, 2, "the bundle holds 0..=invitation");

    // The invitation is open until it is admitted, and the identity document
    // says so without listing the invitations themselves.
    let identity = inviter
        .service
        .identity(alice.clone())
        .await
        .expect("the identity reads");
    assert_eq!(identity.open_invitation_count, 1);
    assert_eq!(identity.principals.len(), 1);

    // Bob's wallet holds Bob's key, so Bob's wallet signs the acceptance.
    let accepted = invitee
        .service
        .accept_invitation(AcceptInvitation {
            identity_id: bob.clone(),
            invitation_bundle: decode(&invited.invitation_bundle_base64),
        })
        .await
        .expect("the bundle folds and the acceptance signs");
    assert_eq!(accepted.ledger_id, alice);
    assert_eq!(accepted.root, RootName::Raw);
    assert_eq!(accepted.invitee, bob);
    assert_eq!(accepted.invitation_event, invited.invitation_event);
    assert_eq!(accepted.controllers.len(), 1);
    assert!(accepted.controllers[0].is_root);
    // Accepting a controller role on a raw-rooted ledger means signing as
    // that identity, and the surface warns before anything is signed.
    assert!(accepted.controller_on_raw_root);
    assert!(
        accepted
            .warning
            .as_deref()
            .expect("a warning beside the flag")
            .contains(alice.as_str())
    );

    // A controller of the ledger admits the file Bob signed.
    let acceptance = decode(&accepted.acceptance_base64);
    let admitted = inviter
        .service
        .admit_acceptance(AdmitAcceptance {
            ledger_id: alice.clone(),
            by: alice.clone(),
            acceptance: acceptance.clone(),
        })
        .await
        .expect("the acceptance lands");
    assert_eq!(admitted.invitee, bob);
    assert_eq!(admitted.role, RoleName::Controller);
    assert_eq!(admitted.acceptance_seq, 2);
    assert_eq!(admitted.invitation_event, invited.invitation_event);
    assert_eq!(admitted.event.payload_kind, "membership_acceptance");

    let memberships = inviter
        .service
        .memberships(alice.clone())
        .await
        .expect("the ledger reads");
    assert_eq!(memberships.principals.len(), 2);
    assert_eq!(memberships.invitations.len(), 1);
    assert_eq!(memberships.invitations[0].status, StatusName::Accepted);
    let bob_principal = memberships
        .principals
        .iter()
        .find(|principal| principal.identity == bob)
        .expect("bob is a principal now");
    assert_eq!(bob_principal.role, RoleName::Controller);
    assert!(!bob_principal.is_root);

    let identity = inviter
        .service
        .identity(alice.clone())
        .await
        .expect("the identity reads");
    assert_eq!(identity.open_invitation_count, 0);
    assert_eq!(identity.principals.len(), 2);

    // The same acceptance a second time is a replay, not a fold rejection.
    let error = inviter
        .service
        .admit_acceptance(AdmitAcceptance {
            ledger_id: alice.clone(),
            by: alice.clone(),
            acceptance,
        })
        .await
        .expect_err("an acceptance is single use on this branch");
    assert_eq!(error.code(), 50);
    assert_eq!(error.reason(), "acceptance_already_used");
    assert_eq!(error.status(), axum::http::StatusCode::CONFLICT);
    assert_eq!(error.details()["at_seq"], serde_json::json!(2));

    // The root of a raw-rooted ledger is not removable, in this POC or later.
    let error = refused(&inviter, &alice, &alice).await;
    assert_eq!(error.code(), 20);
    assert_eq!(error.reason(), "root_not_removable");

    let removed = inviter
        .service
        .remove_membership(RemoveMembership {
            ledger_id: alice.clone(),
            by: alice.clone(),
            target: bob.clone(),
        })
        .await
        .expect("the removal lands");
    assert!(removed.principal_removed);
    assert_eq!(removed.invitation_cancelled, None);
    assert_eq!(removed.removal_seq, 3);
    assert_eq!(removed.event.payload_kind, "membership_removal");

    let memberships = inviter
        .service
        .memberships(alice)
        .await
        .expect("the ledger reads");
    assert_eq!(memberships.principals.len(), 1);
    assert!(memberships.principals[0].is_root);
}

#[tokio::test]
async fn an_open_invitation_is_cancelled_by_removing_the_invitee() {
    let inviter = Wallet::new().await;
    let invitee = Wallet::new().await;
    let alice = inviter.identity("alice").await;
    let bob = invitee.identity("bob").await;

    let invited = inviter
        .service
        .invite(Invite {
            ledger_id: alice.clone(),
            by: alice.clone(),
            role: RoleName::Member,
            invitee_descriptor: invitee.descriptor(&bob),
        })
        .await
        .expect("the invitation lands");

    let removed = inviter
        .service
        .remove_membership(RemoveMembership {
            ledger_id: alice.clone(),
            by: alice.clone(),
            target: bob,
        })
        .await
        .expect("the removal lands");
    assert!(!removed.principal_removed, "nobody was admitted");
    assert_eq!(
        removed.invitation_cancelled,
        Some(invited.invitation_event.clone())
    );

    let memberships = inviter
        .service
        .memberships(alice)
        .await
        .expect("the ledger reads");
    assert_eq!(memberships.invitations[0].status, StatusName::Cancelled);
    assert_eq!(memberships.invitations[0].role, RoleName::Member);
}

#[tokio::test]
async fn only_the_invitee_can_accept_and_only_with_the_key_the_invitation_names() {
    let inviter = Wallet::new().await;
    let invitee = Wallet::new().await;
    let alice = inviter.identity("alice").await;
    let bob = invitee.identity("bob").await;

    let invited = inviter
        .service
        .invite(Invite {
            ledger_id: alice.clone(),
            by: alice.clone(),
            role: RoleName::Controller,
            invitee_descriptor: invitee.descriptor(&bob),
        })
        .await
        .expect("the invitation lands");
    let bundle = decode(&invited.invitation_bundle_base64);

    // Alice's wallet holds Alice's key, and this invitation invites Bob.
    let error = inviter
        .service
        .accept_invitation(AcceptInvitation {
            identity_id: alice,
            invitation_bundle: bundle.clone(),
        })
        .await
        .expect_err("the invitee is not this identity");
    assert_eq!(error.code(), 2);
    assert_eq!(error.reason(), "not_the_invitee");

    // A wallet that does not hold the invitee's key cannot sign for it.
    let stranger = Wallet::new().await;
    let error = stranger
        .service
        .accept_invitation(AcceptInvitation {
            identity_id: bob,
            invitation_bundle: bundle,
        })
        .await
        .expect_err("this home holds no key for the invitee");
    assert_eq!(error.code(), 2);
    assert_eq!(error.reason(), "no_signing_key");
}

#[tokio::test]
async fn an_identity_rooted_ledger_takes_its_founder_as_the_root_principal() {
    let wallet = Wallet::new().await;
    let alice = wallet.identity("alice").await;

    let created = wallet
        .service
        .create_identity(CreateIdentity {
            alias: "acme".to_owned(),
            declared_kind: DeclaredKind::Organization,
            founder: Some(alice.clone()),
        })
        .await
        .expect("the organization is created");
    let acme = created.identity;
    // A ledger founded by an identity holds no key of its own.
    assert_eq!(acme.active_key, None);
    assert_eq!(acme.reserve_commit, None);
    assert_eq!(acme.declared_kind, DeclaredKind::Organization);
    assert_eq!(acme.principals.len(), 1);
    assert_eq!(acme.principals[0].identity, alice);
    assert_eq!(acme.principals[0].role, RoleName::Controller);
    assert!(acme.principals[0].is_root);

    let memberships = wallet
        .service
        .memberships(acme.identity_id.clone())
        .await
        .expect("the ledger reads");
    assert_eq!(memberships.root, RootName::Identity);
    assert!(memberships.invitations.is_empty());

    // The founder signs for it, so a membership event appends under Alice's
    // key without Alice being named anywhere but the root.
    let invitee = Wallet::new().await;
    let bob = invitee.identity("bob").await;
    let invited = wallet
        .service
        .invite(Invite {
            ledger_id: acme.identity_id.clone(),
            by: acme.identity_id.clone(),
            role: RoleName::Member,
            invitee_descriptor: invitee.descriptor(&bob),
        })
        .await
        .expect("the founder may append");
    assert_eq!(invited.invitee, bob);
    assert_eq!(invited.role, RoleName::Member);
}

#[tokio::test]
async fn an_identity_rooted_ledger_cannot_be_invited_anywhere() {
    let wallet = Wallet::new().await;
    let alice = wallet.identity("alice").await;
    let acme = wallet
        .service
        .create_identity(CreateIdentity {
            alias: "acme".to_owned(),
            declared_kind: DeclaredKind::Organization,
            founder: Some(alice.clone()),
        })
        .await
        .expect("the organization is created")
        .identity
        .identity_id;

    let error = wallet
        .service
        .invite(Invite {
            ledger_id: alice.clone(),
            by: alice,
            role: RoleName::Member,
            invitee_descriptor: wallet.descriptor(&acme),
        })
        .await
        .expect_err("an identity-rooted ledger holds no key to sign an acceptance");
    assert_eq!(error.code(), 20);
    assert_eq!(error.reason(), "invitee_holds_no_key");
}

/// The error a removal answers, for a case that must not append.
async fn refused(wallet: &Wallet, ledger: &Id, target: &Id) -> ServiceError {
    wallet
        .service
        .remove_membership(RemoveMembership {
            ledger_id: ledger.clone(),
            by: ledger.clone(),
            target: target.clone(),
        })
        .await
        .expect_err("the fold refuses this removal")
}

/// Two requests that append to one ledger at the same time both land.
///
/// Each one folds the stored chain, signs on the head it read and writes. Run
/// them in parallel without the ledger's append lock and both build on the
/// same head, so the second write takes the sequence the first just took and
/// one intent is lost. They must come back as seq 1 and seq 2, in some order,
/// with three events stored.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn two_concurrent_appends_on_one_ledger_take_the_next_two_sequences() {
    let wallet = Wallet::new().await;
    let alice = wallet.identity("alice").await;
    let bob = wallet.identity("bob").await;
    let carol = wallet.identity("carol").await;

    let (first, second) = tokio::join!(
        wallet.service.add_trust(AddTrust {
            issuer: alice.clone(),
            subject: bob,
        }),
        wallet.service.add_trust(AddTrust {
            issuer: alice.clone(),
            subject: carol,
        }),
    );
    let first = first.expect("the first attestation lands");
    let second = second.expect("the second attestation lands");

    let mut seqs = [first.head_seq, second.head_seq];
    seqs.sort_unstable();
    assert_eq!(seqs, [1, 2], "neither append overwrote the other");
    assert_ne!(first.head_event, second.head_event);

    let ledger = alice.as_str().parse().expect("a rendered id parses");
    let loaded = wallet.core.load(ledger).expect("the ledger loads");
    assert!(loaded.violation.is_none(), "the chain still verifies");
    assert_eq!(loaded.head_seq, 2);
    assert_eq!(loaded.event_count(), 3);
}
