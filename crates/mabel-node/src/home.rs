//! The node home: one directory per node, laid out as proposal 001 section 8
//! names it.
//!
//! ```text
//! node.json                              role, http bind, witnesses, caps
//! node.key                               0600, iroh endpoint secret key
//! identities/<id>/meta.json              alias, kind, created_at_ms
//! identities/<id>/{active,reserve}.key   0600, persons only
//! ledgers/<id>/000000000000.ev           encoded SignedEvent, one per event
//! ledgers/<id>/head.json                 cache: seq, event id, updated_ms
//! ledgers/<id>/meta.json                 provenance: source endpoint, first seen
//! forks/<id>/<seq>-<event_id>.fork       encoded ForkRecord, both events
//! peers.json                             ledger id to EndpointId hints, plus tickets
//! ```

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use iroh_base::SecretKey;
use mabel_core::{IdentityId, LedgerId};
use serde::{Deserialize, Serialize};

use crate::atomic::{DATA_MODE, create_dir, write_atomic};
use crate::config::NodeConfig;
use crate::error::{Result, StorageError, io_at, json_at};
use crate::keys::{generate_secret_key, read_secret_key, write_secret_key};
use crate::ledger::LedgerStore;
use crate::now_ms;
use crate::peers::Peers;

/// Environment variable naming the node home.
pub const HOME_ENV: &str = "MABEL_HOME";

/// Node home under the user's home directory when `$MABEL_HOME` is unset.
pub const DEFAULT_HOME_NAME: &str = ".mabel";

/// Name of the config file.
pub const CONFIG_FILE: &str = "node.json";

/// Name of the node key file.
pub const NODE_KEY_FILE: &str = "node.key";

/// Name of the peer hints file.
pub const PEERS_FILE: &str = "peers.json";

/// Name of the identity key that signs events.
pub const ACTIVE_KEY_FILE: &str = "active.key";

/// Name of the identity key committed at inception and unused in the POC.
pub const RESERVE_KEY_FILE: &str = "reserve.key";

/// Name of the identity metadata file.
pub const IDENTITY_META_FILE: &str = "meta.json";

/// Whether an identity is a person or an org (proposal 001 section 3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IdentityKind {
    /// A person, which holds an active and a reserve key.
    Person,
    /// An org, which holds no keys of its own; its controllers sign for it.
    Org,
}

/// `identities/<id>/meta.json`. Local labelling, never signed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityMeta {
    /// Local alias. Ids are authoritative; an alias is a convenience.
    pub alias: String,
    /// Person or org.
    pub kind: IdentityKind,
    /// When this node created or imported the identity.
    pub created_at_ms: u64,
}

impl IdentityMeta {
    /// Metadata stamped with the current time.
    #[must_use]
    pub fn now(alias: impl Into<String>, kind: IdentityKind) -> Self {
        Self {
            alias: alias.into(),
            kind,
            created_at_ms: now_ms(),
        }
    }
}

/// How a home is opened.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HomeOptions {
    /// The `--allow-insecure-permissions` flag: read a group- or
    /// world-accessible key file instead of failing with exit code 60.
    pub allow_insecure_permissions: bool,
}

/// Resolves the node home: `--home`, then `$MABEL_HOME`, then `~/.mabel`.
///
/// # Errors
///
/// Returns [`StorageError::HomeUnknown`] when none of the three is set.
pub fn resolve_home(explicit: Option<&Path>) -> Result<PathBuf> {
    resolve_home_from(explicit, env::var_os(HOME_ENV), env::var_os("HOME"))
}

/// The resolution rule, with the environment passed in so tests need not
/// mutate it.
fn resolve_home_from(
    explicit: Option<&Path>,
    mabel_home: Option<std::ffi::OsString>,
    user_home: Option<std::ffi::OsString>,
) -> Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path.to_path_buf());
    }
    if let Some(value) = mabel_home.filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(value));
    }
    user_home
        .filter(|value| !value.is_empty())
        .map(|home| PathBuf::from(home).join(DEFAULT_HOME_NAME))
        .ok_or(StorageError::HomeUnknown)
}

/// A node home on disk.
#[derive(Debug, Clone)]
pub struct NodeHome {
    root: PathBuf,
    options: HomeOptions,
}

impl NodeHome {
    /// Opens an existing home.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::NotAHome`] if the directory holds no
    /// `node.json`.
    pub fn open(root: impl Into<PathBuf>, options: HomeOptions) -> Result<Self> {
        let home = Self {
            root: root.into(),
            options,
        };
        if !home.config_path().is_file() {
            return Err(StorageError::NotAHome {
                path: home.root.clone(),
            });
        }
        Ok(home)
    }

    /// Creates a home: the directory tree, `node.json` and a fresh
    /// `node.key`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::HomeExists`] if `node.json` is already there,
    /// [`StorageError::Random`] if the node key cannot be generated and
    /// [`StorageError::Io`] on a failed write.
    pub fn create(
        root: impl Into<PathBuf>,
        config: &NodeConfig,
        options: HomeOptions,
    ) -> Result<Self> {
        let home = Self {
            root: root.into(),
            options,
        };
        if home.config_path().exists() {
            return Err(StorageError::HomeExists {
                path: home.root.clone(),
            });
        }
        create_dir(&home.root)?;
        create_dir(&home.identities_dir())?;
        create_dir(&home.ledgers_dir())?;
        create_dir(&home.forks_dir())?;
        write_secret_key(&home.node_key_path(), &generate_secret_key()?)?;
        home.write_config(config)?;
        Ok(home)
    }

    /// Opens the home, creating it with `config` if `node.json` is absent.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`NodeHome::create`] and [`NodeHome::open`].
    pub fn open_or_create(
        root: impl Into<PathBuf>,
        config: &NodeConfig,
        options: HomeOptions,
    ) -> Result<Self> {
        let root = root.into();
        let home = Self {
            root: root.clone(),
            options,
        };
        if home.config_path().is_file() {
            Self::open(root, options)
        } else {
            Self::create(root, config, options)
        }
    }

    /// The home directory.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// How this home was opened.
    #[must_use]
    pub fn options(&self) -> HomeOptions {
        self.options
    }

    /// `node.json`.
    #[must_use]
    pub fn config_path(&self) -> PathBuf {
        self.root.join(CONFIG_FILE)
    }

    /// `node.key`.
    #[must_use]
    pub fn node_key_path(&self) -> PathBuf {
        self.root.join(NODE_KEY_FILE)
    }

    /// `peers.json`.
    #[must_use]
    pub fn peers_path(&self) -> PathBuf {
        self.root.join(PEERS_FILE)
    }

    /// `identities/`.
    #[must_use]
    pub fn identities_dir(&self) -> PathBuf {
        self.root.join("identities")
    }

    /// `ledgers/`.
    #[must_use]
    pub fn ledgers_dir(&self) -> PathBuf {
        self.root.join("ledgers")
    }

    /// `forks/`.
    #[must_use]
    pub fn forks_dir(&self) -> PathBuf {
        self.root.join("forks")
    }

    /// `identities/<id>/`.
    #[must_use]
    pub fn identity_dir(&self, identity: IdentityId) -> PathBuf {
        self.identities_dir().join(identity.to_string())
    }

    /// Loads `node.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Json`] for a malformed file, an unknown field
    /// or an unknown value, and [`StorageError::Io`] if it cannot be read.
    pub fn config(&self) -> Result<NodeConfig> {
        let path = self.config_path();
        let bytes = fs::read(&path).map_err(io_at(&path))?;
        NodeConfig::from_json(&bytes).map_err(json_at(&path))
    }

    /// Writes `node.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if the write fails.
    pub fn write_config(&self, config: &NodeConfig) -> Result<()> {
        create_dir(&self.root)?;
        let path = self.config_path();
        let bytes = config.to_json().map_err(json_at(&path))?;
        write_atomic(&path, &bytes, DATA_MODE)
    }

    /// Reads the Iroh endpoint secret key.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InsecurePermissions`] (exit code 60) if the
    /// file is group- or world-accessible and the home was not opened with
    /// `allow_insecure_permissions`.
    pub fn node_key(&self) -> Result<SecretKey> {
        read_secret_key(
            &self.node_key_path(),
            self.options.allow_insecure_permissions,
        )
    }

    /// Creates `identities/<id>/meta.json`.
    ///
    /// Keys are written separately, since an org holds none.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if the directory or the file cannot be
    /// written.
    pub fn create_identity(&self, identity: IdentityId, meta: &IdentityMeta) -> Result<()> {
        let dir = self.identity_dir(identity);
        create_dir(&dir)?;
        self.write_identity_meta(identity, meta)
    }

    /// Writes `identities/<id>/meta.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if the write fails.
    pub fn write_identity_meta(&self, identity: IdentityId, meta: &IdentityMeta) -> Result<()> {
        let dir = self.identity_dir(identity);
        create_dir(&dir)?;
        let path = dir.join(IDENTITY_META_FILE);
        let mut bytes = serde_json::to_vec_pretty(meta).map_err(json_at(&path))?;
        bytes.push(b'\n');
        write_atomic(&path, &bytes, DATA_MODE)
    }

    /// Reads `identities/<id>/meta.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::UnknownIdentity`] if the home holds no such
    /// identity and [`StorageError::Json`] if the file is malformed.
    pub fn identity_meta(&self, identity: IdentityId) -> Result<IdentityMeta> {
        let path = self.identity_dir(identity).join(IDENTITY_META_FILE);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(StorageError::UnknownIdentity { identity });
            }
            Err(error) => return Err(io_at(&path)(error)),
        };
        serde_json::from_slice(&bytes).map_err(json_at(&path))
    }

    /// Writes a person's active and reserve keys at mode 0600.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if a write fails.
    pub fn write_identity_keys(
        &self,
        identity: IdentityId,
        active: &SecretKey,
        reserve: &SecretKey,
    ) -> Result<()> {
        let dir = self.identity_dir(identity);
        create_dir(&dir)?;
        write_secret_key(&dir.join(ACTIVE_KEY_FILE), active)?;
        write_secret_key(&dir.join(RESERVE_KEY_FILE), reserve)
    }

    /// Reads the key that signs this identity's events.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::InsecurePermissions`] (exit code 60) for a
    /// group- or world-accessible file, and [`StorageError::Io`] if the key
    /// is absent, which is the case for every org.
    pub fn identity_active_key(&self, identity: IdentityId) -> Result<SecretKey> {
        self.read_identity_key(identity, ACTIVE_KEY_FILE)
    }

    /// Reads the key committed at inception and unused in the POC.
    ///
    /// # Errors
    ///
    /// Returns the errors of [`NodeHome::identity_active_key`].
    pub fn identity_reserve_key(&self, identity: IdentityId) -> Result<SecretKey> {
        self.read_identity_key(identity, RESERVE_KEY_FILE)
    }

    fn read_identity_key(&self, identity: IdentityId, file: &str) -> Result<SecretKey> {
        let path = self.identity_dir(identity).join(file);
        read_secret_key(&path, self.options.allow_insecure_permissions)
    }

    /// Every identity in the home, sorted.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if `identities/` cannot be listed.
    pub fn identities(&self) -> Result<Vec<IdentityId>> {
        list_ids(&self.identities_dir())
    }

    /// Every ledger in the home, sorted.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if `ledgers/` cannot be listed.
    pub fn ledgers(&self) -> Result<Vec<LedgerId>> {
        list_ids(&self.ledgers_dir())
    }

    /// The store for one ledger. Nothing is read or created until it is used.
    #[must_use]
    pub fn ledger(&self, ledger: LedgerId) -> LedgerStore {
        LedgerStore::new(
            ledger,
            self.ledgers_dir().join(ledger.to_string()),
            self.forks_dir().join(ledger.to_string()),
        )
    }

    /// Loads `peers.json`, which defaults to empty.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Json`] if the file is malformed.
    pub fn peers(&self) -> Result<Peers> {
        let path = self.peers_path();
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).map_err(json_at(&path)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Peers::default()),
            Err(error) => Err(io_at(&path)(error)),
        }
    }

    /// Writes `peers.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError::Io`] if the write fails.
    pub fn write_peers(&self, peers: &Peers) -> Result<()> {
        create_dir(&self.root)?;
        let path = self.peers_path();
        let mut bytes = serde_json::to_vec_pretty(peers).map_err(json_at(&path))?;
        bytes.push(b'\n');
        write_atomic(&path, &bytes, DATA_MODE)
    }
}

/// Directory names under `dir` that parse as ids, sorted.
fn list_ids<T: std::str::FromStr + Ord>(dir: &Path) -> Result<Vec<T>> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_at(dir)(error)),
    };
    let mut ids = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io_at(dir))?;
        if !entry.path().is_dir() {
            continue;
        }
        if let Ok(id) = entry.file_name().to_string_lossy().parse::<T>() {
            ids.push(id);
        }
    }
    ids.sort_unstable();
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use mabel_core::IdentityId;

    use super::{
        ACTIVE_KEY_FILE, HomeOptions, IdentityKind, IdentityMeta, NodeHome, RESERVE_KEY_FILE,
        resolve_home,
    };
    use crate::config::{NodeConfig, NodeRole, RelayMode};

    fn home(root: &Path) -> NodeHome {
        NodeHome::create(root, &NodeConfig::default(), HomeOptions::default()).unwrap()
    }

    #[test]
    fn create_lays_out_the_section_8_tree() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());

        assert!(home.config_path().is_file());
        assert!(home.node_key_path().is_file());
        assert!(home.identities_dir().is_dir());
        assert!(home.ledgers_dir().is_dir());
        assert!(home.forks_dir().is_dir());
        assert_eq!(home.config().unwrap(), NodeConfig::default());
        assert!(home.identities().unwrap().is_empty());
        assert!(home.ledgers().unwrap().is_empty());
    }

    #[test]
    fn a_second_create_refuses_an_existing_home() {
        let dir = tempfile::tempdir().unwrap();
        let first = home(dir.path());
        let key = first.node_key().unwrap().to_bytes();

        let error = NodeHome::create(dir.path(), &NodeConfig::default(), HomeOptions::default())
            .expect_err("the home is already there");
        assert_eq!(error.exit_code(), 2);

        let reopened =
            NodeHome::open_or_create(dir.path(), &NodeConfig::default(), HomeOptions::default())
                .unwrap();
        assert_eq!(
            reopened.node_key().unwrap().to_bytes(),
            key,
            "the node key survives"
        );
    }

    #[test]
    fn opening_a_directory_without_node_json_fails() {
        let dir = tempfile::tempdir().unwrap();
        let error = NodeHome::open(dir.path(), HomeOptions::default()).expect_err("not a home");
        assert_eq!(error.exit_code(), 2);
    }

    #[test]
    fn config_survives_a_write_and_reload() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        let config = NodeConfig {
            role: NodeRole::Witness,
            relay: RelayMode::Disabled,
            ..NodeConfig::default()
        };
        home.write_config(&config).unwrap();
        assert_eq!(home.config().unwrap(), config);
    }

    #[test]
    fn a_node_json_with_an_unknown_relay_value_fails_to_load() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        std::fs::write(home.config_path(), br#"{"relay": "sometimes"}"#).unwrap();
        let error = home.config().expect_err("sometimes is not a relay mode");
        assert_eq!(error.exit_code(), 10);
        assert!(error.to_string().contains("node.json"), "{error}");
    }

    #[test]
    fn identity_keys_are_written_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        let identity = IdentityId::from_bytes([2u8; 32]);
        let active = crate::keys::generate_secret_key().unwrap();
        let reserve = crate::keys::generate_secret_key().unwrap();

        home.create_identity(identity, &IdentityMeta::now("ada", IdentityKind::Person))
            .unwrap();
        home.write_identity_keys(identity, &active, &reserve)
            .unwrap();

        let meta = home.identity_meta(identity).unwrap();
        assert_eq!(meta.alias, "ada");
        assert_eq!(meta.kind, IdentityKind::Person);
        assert_eq!(
            home.identity_active_key(identity).unwrap().to_bytes(),
            active.to_bytes()
        );
        assert_eq!(
            home.identity_reserve_key(identity).unwrap().to_bytes(),
            reserve.to_bytes()
        );
        assert_eq!(home.identities().unwrap(), vec![identity]);
    }

    #[test]
    fn an_org_identity_holds_metadata_and_no_keys() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        let org = IdentityId::from_bytes([3u8; 32]);
        home.create_identity(org, &IdentityMeta::now("acme", IdentityKind::Org))
            .unwrap();

        assert_eq!(home.identity_meta(org).unwrap().kind, IdentityKind::Org);
        assert!(home.identity_active_key(org).is_err());
        assert!(!home.identity_dir(org).join(ACTIVE_KEY_FILE).exists());
        assert!(!home.identity_dir(org).join(RESERVE_KEY_FILE).exists());
    }

    #[test]
    fn an_unknown_identity_is_named_in_the_error() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        let identity = IdentityId::from_bytes([8u8; 32]);
        let error = home.identity_meta(identity).expect_err("not in this home");
        assert!(error.to_string().contains(&identity.to_string()), "{error}");
    }

    #[test]
    fn the_node_key_is_distinct_from_every_identity_key() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        let identity = IdentityId::from_bytes([2u8; 32]);
        let active = crate::keys::generate_secret_key().unwrap();
        let reserve = crate::keys::generate_secret_key().unwrap();
        home.write_identity_keys(identity, &active, &reserve)
            .unwrap();

        let node_key = home.node_key().unwrap().to_bytes();
        assert_ne!(node_key, active.to_bytes());
        assert_ne!(node_key, reserve.to_bytes());
        assert_ne!(active.to_bytes(), reserve.to_bytes());
    }

    #[test]
    fn peers_default_to_empty_and_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        assert_eq!(home.peers().unwrap(), crate::peers::Peers::default());

        let mut peers = crate::peers::Peers::default();
        peers.add_hint(
            IdentityId::from_bytes([1u8; 32]),
            iroh_base::SecretKey::from_bytes(&[7u8; 32]).public(),
        );
        home.write_peers(&peers).unwrap();
        assert_eq!(home.peers().unwrap(), peers);
    }

    #[test]
    fn resolve_home_prefers_the_explicit_path() {
        let explicit = Path::new("/tmp/explicit-home");
        assert_eq!(resolve_home(Some(explicit)).unwrap(), explicit);
    }

    #[test]
    fn home_resolution_falls_back_from_the_flag_to_the_env_to_the_user_home() {
        use std::ffi::OsString;

        let flag = Path::new("/flag");
        let env = || Some(OsString::from("/env"));
        let user = || Some(OsString::from("/user"));

        assert_eq!(
            super::resolve_home_from(Some(flag), env(), user()).unwrap(),
            Path::new("/flag")
        );
        assert_eq!(
            super::resolve_home_from(None, env(), user()).unwrap(),
            Path::new("/env")
        );
        assert_eq!(
            super::resolve_home_from(None, None, user()).unwrap(),
            Path::new("/user/.mabel")
        );
        assert_eq!(
            super::resolve_home_from(None, Some(OsString::new()), user()).unwrap(),
            Path::new("/user/.mabel"),
            "an empty MABEL_HOME is not a home"
        );
        let error = super::resolve_home_from(None, None, None).expect_err("nothing to resolve");
        assert_eq!(error.exit_code(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn directories_are_0700_and_key_files_are_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        let identity = IdentityId::from_bytes([2u8; 32]);
        home.write_identity_keys(
            identity,
            &crate::keys::generate_secret_key().unwrap(),
            &crate::keys::generate_secret_key().unwrap(),
        )
        .unwrap();

        let mode = |path: std::path::PathBuf| {
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777
        };
        for directory in [
            home.identities_dir(),
            home.ledgers_dir(),
            home.forks_dir(),
            home.identity_dir(identity),
        ] {
            assert_eq!(mode(directory.clone()), 0o700, "{}", directory.display());
        }
        for key in [
            home.node_key_path(),
            home.identity_dir(identity).join(ACTIVE_KEY_FILE),
            home.identity_dir(identity).join(RESERVE_KEY_FILE),
        ] {
            assert_eq!(mode(key.clone()), 0o600, "{}", key.display());
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_world_readable_node_key_is_refused_unless_the_flag_is_set() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        std::fs::set_permissions(home.node_key_path(), std::fs::Permissions::from_mode(0o644))
            .unwrap();

        let error = home.node_key().expect_err("0644 is refused");
        assert!(error.is_insecure_permissions());
        assert_eq!(error.exit_code(), 60);
        assert!(error.to_string().contains("node.key"), "{error}");

        let lenient = NodeHome::open(
            dir.path(),
            HomeOptions {
                allow_insecure_permissions: true,
            },
        )
        .unwrap();
        lenient.node_key().expect("the flag allows the read");
    }

    #[cfg(unix)]
    #[test]
    fn rewriting_a_loose_identity_key_tightens_it_to_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let home = home(dir.path());
        let identity = IdentityId::from_bytes([2u8; 32]);
        let active = crate::keys::generate_secret_key().unwrap();
        let reserve = crate::keys::generate_secret_key().unwrap();
        home.write_identity_keys(identity, &active, &reserve)
            .unwrap();

        let path = home.identity_dir(identity).join(ACTIVE_KEY_FILE);
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o666)).unwrap();
        assert!(home.identity_active_key(identity).is_err());

        home.write_identity_keys(identity, &active, &reserve)
            .unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        home.identity_active_key(identity).expect("readable again");
    }
}
