//! The node home a command runs against, and the alias table.
//!
//! Aliases are local labels in `identities/<id>/meta.json` and are never
//! signed; the id is authoritative (proposal 001 section 9). Every name a
//! command takes goes through [`Context::resolve`], so one spelling rule
//! covers every flag.

use std::path::{Path, PathBuf};

use iroh_base::{EndpointId, SecretKey};
use mabel_core::{IdentityId, LedgerId};
use mabel_node::{HomeOptions, LedgerStore, NodeConfig, NodeHome, resolve_home};

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

    /// Resolves an id or a local alias to an id.
    ///
    /// An id resolves to itself whether or not this home holds it, which is
    /// what lets `--subject` name someone else's identity.
    ///
    /// # Errors
    ///
    /// Returns code 2 with reason `unknown_alias` when no local identity
    /// carries the name.
    pub fn resolve(&self, name: &str) -> Result<IdentityId> {
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
            format!("identity {identity} is not in this home"),
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
                format!("this home holds no key that may sign for {identity}"),
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
            "ledger {ledger} is stored here read-only: no identity in this home \
             is one of its controllers"
        ),
    )
    .with_detail("ledger_id", ledger.to_string())
    .with_detail("identity", ledger.to_string())
}
