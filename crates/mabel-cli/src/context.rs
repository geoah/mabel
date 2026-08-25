//! The node home a command runs against, and the alias table.
//!
//! Aliases are local labels in `identities/<id>/meta.json` and are never
//! signed; the id is authoritative (proposal 001 section 9). Every name a
//! command takes goes through [`Context::resolve`], so one spelling rule
//! covers every flag.

use std::path::{Path, PathBuf};

use iroh_base::{EndpointId, SecretKey};
use mabel_core::{IdentityId, LedgerId, MabelLink};
use mabel_node::api::documents::{Identity, Verification};
use mabel_node::verification::VerificationStore;
use mabel_node::wallet::{contact_document, verification_document};
use mabel_node::{
    ContactStore, HomeOptions, LedgerStore, NodeConfig, NodeHome, now_ms, resolve_home,
};

use crate::error::{CliError, Result};
use crate::ids;
use crate::ledger::Loaded;

/// The home, opened once per invocation.
pub struct Context {
    home: NodeHome,
}

impl Context {
    /// Opens the home named by `--home`, `$MABEL_HOME` or `~/.mabel`,
    /// creating it if it is not there yet.
    ///
    /// # Errors
    ///
    /// Returns code 2 when no home can be resolved and code 1 when the
    /// directory cannot be written.
    pub fn open(explicit: Option<&Path>, allow_insecure_permissions: bool) -> Result<Self> {
        let root = resolve_home(explicit)?;
        let options = HomeOptions {
            allow_insecure_permissions,
        };
        let home = NodeHome::open_or_create(root, &NodeConfig::default(), options)?;
        Ok(Self { home })
    }

    /// The home.
    #[must_use]
    pub fn home(&self) -> &NodeHome {
        &self.home
    }

    /// The home directory, which every path in an error is spelled relative
    /// to.
    #[must_use]
    pub fn root(&self) -> PathBuf {
        self.home.root().to_path_buf()
    }

    /// Resolves an id, a local alias or a `mabel://` link to an id.
    ///
    /// An id resolves to itself whether or not this home holds it, which is
    /// what lets `--subject` name someone else's identity. A link resolves to
    /// the identity it names, and a link that also names endpoints is refused:
    /// this slot takes one identity and dials nothing, so obeying the identity
    /// and dropping the endpoints would leave a flag that looks obeyed and is
    /// not. A command that does dial reads them through
    /// [`Context::resolve_hinted`].
    ///
    /// # Errors
    ///
    /// Returns code 2 with reason `unknown_alias` when no local identity
    /// carries the name, and `invalid_mabel_link` for a string that means to be
    /// a link and is not, and for a link that names endpoints.
    pub fn resolve(&self, name: &str) -> Result<IdentityId> {
        if MabelLink::looks_like_link(name) {
            let link = parse_link(name)?;
            if !link.endpoints().is_empty() {
                return Err(endpoints_not_accepted(name));
            }
            return Ok(link.identity());
        }
        if let Ok(id) = name.parse::<IdentityId>() {
            return Ok(id);
        }
        for identity in self.home.identities()? {
            if self.home.identity_meta(identity)?.alias == name {
                return Ok(identity);
            }
        }
        Err(
            CliError::usage("unknown_alias", format!("no identity here is named {name}"))
                .with_detail("alias", name),
        )
    }

    /// Resolves a name and keeps the endpoints a link hinted at.
    ///
    /// The hints apply to the subject that is fetched, and to nothing else: an
    /// alias and a bare id hint at nothing, so a caller that dials falls back
    /// to whatever it would have used (proposal 006 section 7).
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Context::resolve`].
    pub fn resolve_hinted(&self, name: &str) -> Result<(IdentityId, Vec<EndpointId>)> {
        if MabelLink::looks_like_link(name) {
            let link = parse_link(name)?;
            return Ok((link.identity(), link.endpoints().to_vec()));
        }
        Ok((self.resolve(name)?, Vec::new()))
    }

    /// Resolves a name that must be an identity this home signs for, refusing a
    /// link on `flag` that also names endpoints.
    ///
    /// A local signer is reached from this home, so there is nothing to dial and
    /// the hints would do nothing. This is an id-only slot like every other, so
    /// the endpoints are refused rather than dropped: a flag that looks obeyed
    /// and is not is worse than a refusal (proposal 006 section 7).
    ///
    /// `flag` is the flag the value arrived on, which the refusal names.
    ///
    /// # Errors
    ///
    /// Returns code 2 with reason `invalid_mabel_link` for a link that names
    /// endpoints, and the errors of [`Context::resolve_local`].
    pub fn resolve_local_hinted(&self, name: &str, flag: &str) -> Result<IdentityId> {
        if MabelLink::looks_like_link(name) && !parse_link(name)?.endpoints().is_empty() {
            return Err(endpoints_not_accepted(name).with_detail("parameter", flag));
        }
        self.resolve_local(name)
    }

    /// Resolves a name that must be an identity this home can act as.
    ///
    /// Three kinds of ledger pass: one that holds its own keys, one founded
    /// here, and one fetched whose CONTROLLER set named a local key, which
    /// `sync fetch` linked with `controlled_by` (ticket 031). A ledger stored
    /// with no such link is read-only and is refused by name.
    ///
    /// # Errors
    ///
    /// Returns code 2 with reason `not_locally_controlled` for a ledger this
    /// home stores but holds no controlling key for, code 2 with reason
    /// `unknown_identity` for an id this home does not hold at all, and the
    /// errors of [`Context::resolve`].
    pub fn resolve_local(&self, name: &str) -> Result<IdentityId> {
        let identity = self.resolve(name)?;
        if self.home.can_sign_for(identity) {
            return Ok(identity);
        }
        if self.home.identity_dir(identity).is_dir() || self.holds(identity) {
            return Err(not_locally_controlled(identity));
        }
        Err(CliError::usage(
            "unknown_identity",
            format!("identity {} is not in this home", ids::shown(identity)),
        )
        .with_detail("identity", identity.to_string()))
    }

    /// Whether this home stores any event of `ledger`.
    #[must_use]
    pub fn holds(&self, ledger: LedgerId) -> bool {
        self.store(ledger).head().is_ok_and(|head| head.is_some())
    }

    /// The alias recorded for an identity, or its id when the home records
    /// none.
    #[must_use]
    pub fn alias(&self, identity: IdentityId) -> String {
        self.home
            .identity_meta(identity)
            .map_or_else(|_| identity.to_string(), |meta| meta.alias)
    }

    /// The store for one ledger.
    #[must_use]
    pub fn store(&self, ledger: LedgerId) -> LedgerStore {
        self.home.ledger(ledger)
    }

    /// Folds one stored ledger.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Loaded::open`].
    pub fn load(&self, ledger: LedgerId) -> Result<Loaded> {
        Loaded::open(&self.store(ledger))
    }

    /// The private contact notes of this home.
    #[must_use]
    pub fn contacts(&self) -> ContactStore {
        ContactStore::new(&self.home)
    }

    /// The advisory hostname cache of this home.
    #[must_use]
    pub fn verifications(&self) -> VerificationStore {
        VerificationStore::new(&self.home)
    }

    /// The identity document with the two caches in the home folded in.
    ///
    /// Cache-only, on every command: `mabel identity list` never dials a
    /// resolver, which is the same rule `GET /api/identities` follows
    /// (proposal 003 section 2).
    ///
    /// # Errors
    ///
    /// Returns the errors of reading `contacts/` and `verification/`.
    pub fn identity_document(&self, identity: IdentityId) -> Result<Identity> {
        let mut document = self.load(identity)?.identity_document(self.alias(identity));
        if let Some(hostname) = document
            .profile
            .as_ref()
            .and_then(|profile| profile.hostname.clone())
        {
            document.verification = match self.verifications().read_bound(identity, &hostname)? {
                Some(entry) => verification_document(&entry, now_ms()),
                None => Verification::unchecked(&hostname),
            };
        }
        document.contact = self
            .contacts()
            .read(identity)?
            .as_ref()
            .map(contact_document);
        Ok(document)
    }

    /// The key that signs for an identity, following the `controlled_by` link
    /// of an identity-rooted ledger.
    ///
    /// # Errors
    ///
    /// Returns code 60 for a group- or world-accessible key file and code 2
    /// when neither this identity nor the identity it names holds a key.
    pub fn signing_key(&self, identity: IdentityId) -> Result<SecretKey> {
        self.home.identity_active_key(identity).map_err(|error| {
            if error.is_insecure_permissions() {
                return CliError::from(error);
            }
            CliError::usage(
                "no_signing_key",
                format!(
                    "this home holds no key that may sign for {}",
                    ids::shown(identity)
                ),
            )
            .with_detail("identity", identity.to_string())
        })
    }

    /// This node's Iroh endpoint id, which is the source a local verification
    /// reports (flag R, proposal 001 section 6).
    ///
    /// # Errors
    ///
    /// Returns code 60 for a group- or world-accessible `node.key`.
    pub fn endpoint_id(&self) -> Result<EndpointId> {
        Ok(self.home.node_key()?.public())
    }

    /// The endpoint id as a document renders it.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`Context::endpoint_id`].
    pub fn source(&self) -> Result<mabel_node::api::documents::Id> {
        Ok(ids::key(&self.endpoint_id()?))
    }
}

/// One `mabel://` link, refused whole and with one reason (proposal 006
/// section 7).
///
/// # Errors
///
/// Returns code 2 with reason `invalid_mabel_link`, `details.input` holding the
/// string as it was typed.
pub fn parse_link(input: &str) -> Result<MabelLink> {
    MabelLink::parse(input).map_err(|error| {
        CliError::usage(
            error.reason(),
            format!("{input} is not a mabel link: {error}"),
        )
        .with_detail("input", input)
        .with_detail("detail", error.clause())
    })
}

/// The refusal an id-only slot answers a link that also names endpoints.
///
/// The slot takes one identity and dials nothing, so there is nowhere for the
/// endpoints to be used. Dropping them silently would leave a link that looks
/// obeyed and is not, which is the failure proposal 006 section 7 names. The
/// reason is the link grammar's own [`mabel_core::INVALID_MABEL_LINK`], and
/// `details.input` carries the string as it was typed, the shape [`parse_link`]
/// already uses.
#[must_use]
fn endpoints_not_accepted(input: &str) -> CliError {
    CliError::usage(
        mabel_core::INVALID_MABEL_LINK,
        format!("{input} names endpoints, and this value takes one identity: give the identity on its own"),
    )
    .with_detail("input", input)
    .with_detail("detail", "this value takes one identity and names no endpoint")
}

/// The refusal a ledger this home stores but cannot sign for answers.
///
/// A fetch stores any chain that verifies; only a chain naming a local key a
/// controller is linked to a signing identity (ticket 031). Everything else is
/// a read-only copy, and saying so names the ledger rather than pretending the
/// home never heard of it.
#[must_use]
pub fn not_locally_controlled(ledger: LedgerId) -> CliError {
    CliError::usage(
        "not_locally_controlled",
        format!(
            "ledger {} is stored here read-only: no identity in this home \
             is one of its controllers",
            ids::shown(ledger)
        ),
    )
    .with_detail("ledger_id", ledger.to_string())
    .with_detail("identity", ledger.to_string())
}

#[cfg(test)]
mod tests {
    use super::Context;
    use iroh_base::SecretKey;
    use mabel_core::{IdentityId, MabelLink};

    /// A home in a fresh directory, which every one of these cases resolves
    /// against without holding a single identity.
    fn context() -> (tempfile::TempDir, Context) {
        let root = tempfile::tempdir().expect("a temporary directory");
        let context = Context::open(Some(root.path()), false).expect("the home opens");
        (root, context)
    }

    fn identity() -> IdentityId {
        IdentityId::from_bytes([0x11; 32])
    }

    /// `mabel://<id>?endpoints=<endpoint>`, the string every case below hands
    /// to an id-only slot.
    fn link_with_endpoints() -> String {
        let endpoint = SecretKey::from_bytes(&[7u8; 32]).public();
        MabelLink::new(identity(), &[endpoint])
            .expect("one endpoint fits the link cap")
            .to_string()
    }

    #[test]
    fn an_id_only_slot_refuses_a_link_that_names_endpoints() {
        let (_root, context) = context();
        let text = link_with_endpoints();

        for error in [
            context.resolve(&text).expect_err("resolve refuses it"),
            context
                .resolve_local_hinted(&text, "--identity")
                .expect_err("resolve_local_hinted refuses it"),
        ] {
            assert_eq!(error.exit_code(), 2);
            let document = error.to_document();
            assert_eq!(
                document["details"]["reason"],
                mabel_core::INVALID_MABEL_LINK
            );
            assert_eq!(
                document["details"]["input"], text,
                "the refusal names the string as it was typed"
            );
        }
    }

    #[test]
    fn an_id_only_slot_takes_a_link_that_names_no_endpoint() {
        let (_root, context) = context();
        let bare = MabelLink::new(identity(), &[])
            .expect("no endpoint is a link")
            .to_string();
        assert_eq!(context.resolve(&bare).expect("a bare link"), identity());
        assert_eq!(
            context
                .resolve(&identity().to_string())
                .expect("a bare id resolves to itself"),
            identity()
        );
    }

    #[test]
    fn a_slot_that_dials_still_keeps_the_endpoints_a_link_names() {
        let (_root, context) = context();
        let endpoint = SecretKey::from_bytes(&[7u8; 32]).public();
        let (resolved, hints) = context
            .resolve_hinted(&link_with_endpoints())
            .expect("resolve_hinted takes endpoints");
        assert_eq!(resolved, identity());
        assert_eq!(hints, [endpoint]);
    }
}
