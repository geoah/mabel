//! Starting and stopping a node: the Iroh endpoint, the sync server and the
//! HTTP API plus the UI, over one home and one store (proposal 006 section 8).
//!
//! One runtime serves every node. What a node can do is read from what it
//! holds: the identities under `identities/` are what it signs for, and
//! `node.json.witness_for` is who it accepts strangers' pushes on behalf of.
//! Neither gates a route and neither picks a store.
//!
//! [`NodeRuntime::start`] does everything that can fail and reports where the
//! node ended up listening, so a caller prints the endpoint id and both
//! addresses before the serve loop begins. [`NodeRuntime::serve`] then runs
//! until ctrl-c and shuts both listeners down.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use iroh::protocol::Router as IrohRouter;
use iroh::{EndpointAddr, EndpointId};
use mabel_net::{ALPN, LedgerProtocol};
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::api::{ApiOptions, UiSource, node_router};
use crate::endpoint::bind_endpoint;
use crate::home::NodeHome;
use crate::storage::{AdmissionPolicy, LedgerStorage, StorageCaps};
use crate::store::NodeStore;
use crate::wallet::core::WalletCore;
use crate::wallet::service::NodeApiService;
use crate::wallet::sync::WalletSync;

/// What `mabel serve` was told on the command line.
#[derive(Debug, Default)]
pub struct NodeOptions {
    /// `--http <addr>`, overriding `node.json`'s `http_bind`.
    pub http_bind: Option<SocketAddr>,
    /// `--iroh-port <n>`, overriding the ephemeral UDP port.
    pub iroh_port: Option<u16>,
    /// Addresses from `--peer <ticket>`, seeded into the address lookup. An
    /// address hint, never authorization (proposal 001 section 4).
    pub peers: Vec<EndpointAddr>,
    /// Where the UI bundle comes from.
    pub ui: UiSource,
    /// `--allow-host <host[:port]>`, adding to `node.json`'s `allowed_hosts`
    /// rather than replacing it (decision 018).
    pub allowed_hosts: Vec<String>,
    /// The caps to enforce, `node.json`'s and proposal 001 section 5's unless a
    /// test shrinks them.
    pub caps: Option<StorageCaps>,
}

/// A node that has bound both listeners and not yet begun serving.
#[derive(Debug)]
pub struct NodeRuntime {
    endpoint_id: EndpointId,
    http_address: SocketAddr,
    iroh_addresses: Vec<SocketAddr>,
    warning: Option<String>,
    role_notice: Option<String>,
    allowed_hosts: Vec<String>,
    listener: TcpListener,
    app: axum::Router,
    iroh: IrohRouter,
    core: Arc<WalletCore>,
    storage: Arc<LedgerStorage>,
}

impl NodeRuntime {
    /// Reads the home, builds the index, binds the Iroh endpoint and the HTTP
    /// listener.
    ///
    /// # Errors
    ///
    /// Returns the errors of reading `node.json` and `node.key`, of building
    /// the index from the event files, and of binding either listener.
    pub async fn start(home: NodeHome, options: NodeOptions) -> anyhow::Result<Self> {
        let config = home.config()?;
        // `role` is recognised and read by nothing (proposal 006 section 8).
        // The field stays so every `node.json` written before this release still
        // loads under `deny_unknown_fields`; this is the only thing that reads
        // it, once per start, and it names the file, the key and the fix.
        let role_notice = declares_role(&home).then(|| {
            format!(
                "{} carries the key role, which is recognised and read by nothing: what this \
                 node can do is the identities it holds and witness_for. Delete the line",
                home.config_path().display()
            )
        });
        if let Some(notice) = &role_notice {
            warn!("{notice}");
        }
        let secret = home.node_key()?;
        let endpoint_id = secret.public();

        let caps = options
            .caps
            .unwrap_or_else(|| StorageCaps::from_config(&config));
        // Which identities this home witnesses for, and whether the retired
        // tag-11 clause may admit a push, are read once at startup: an operator
        // who edits either restarts the node, exactly as they do for every
        // other `node.json` value (proposal 006 section 4).
        let policy = AdmissionPolicy::from_config(&config);
        let opened = home.clone();
        let storage = Arc::new(
            tokio::task::spawn_blocking(move || {
                LedgerStorage::open(opened, endpoint_id, caps, policy)
            })
            .await??,
        );

        let endpoint =
            bind_endpoint(config.relay, secret, options.iroh_port, &options.peers).await?;
        let iroh_addresses = endpoint.bound_sockets();
        let iroh = IrohRouter::builder(endpoint.clone())
            .accept(
                ALPN,
                LedgerProtocol::new(Arc::new(NodeStore::new(storage.clone()))),
            )
            .spawn();

        let bound = crate::api::bind::bind(options.http_bind.unwrap_or(config.http_bind)).await?;
        // The core writes and the index serves reads, so the core notes every
        // ledger it touches: one store (proposal 006 section 8).
        let core = Arc::new(WalletCore::new(home).with_index(storage.clone()));
        let service = Arc::new(NodeApiService::new(
            core.clone(),
            storage.clone(),
            WalletSync::new(endpoint),
            bound.address,
            config.relay,
        ));
        // `node.json` and `--allow-host` both contribute; the rules drop the
        // repeats and the loopback spellings (decision 018).
        let api = ApiOptions::new(bound.address)
            .with_ui(options.ui)
            .with_allowed_hosts(config.allowed_hosts)
            .with_allowed_hosts(options.allowed_hosts);
        let allowed_hosts = api.loopback_rules().allowed_hosts().to_vec();
        let app = node_router(service, &api);

        info!(%endpoint_id, http = %bound.address, "the node is listening");
        Ok(Self {
            endpoint_id,
            http_address: bound.address,
            iroh_addresses,
            warning: bound.warning,
            role_notice,
            allowed_hosts,
            listener: bound.listener,
            app,
            iroh,
            core,
            storage,
        })
    }

    /// This node's Iroh endpoint id, which a peer dials to reach it.
    #[must_use]
    pub fn endpoint_id(&self) -> EndpointId {
        self.endpoint_id
    }

    /// Where the HTTP API listens, with port 0 resolved.
    #[must_use]
    pub fn http_address(&self) -> SocketAddr {
        self.http_address
    }

    /// The UDP sockets the Iroh endpoint bound.
    #[must_use]
    pub fn iroh_addresses(&self) -> &[SocketAddr] {
        &self.iroh_addresses
    }

    /// The warning for an HTTP bind that is not loopback, if there is one.
    #[must_use]
    pub fn warning(&self) -> Option<&str> {
        self.warning.as_deref()
    }

    /// The one sentence a `node.json` carrying `role` gets, or `None` when the
    /// file carries no such key (proposal 006 section 8).
    #[must_use]
    pub fn role_notice(&self) -> Option<&str> {
        self.role_notice.as_deref()
    }

    /// The hosts this node accepts beyond loopback, `node.json` and
    /// `--allow-host` merged (decision 018).
    #[must_use]
    pub fn allowed_hosts(&self) -> &[String] {
        &self.allowed_hosts
    }

    /// The home this node serves, with every append rule over it.
    #[must_use]
    pub fn core(&self) -> &Arc<WalletCore> {
        &self.core
    }

    /// The store both surfaces share.
    #[must_use]
    pub fn storage(&self) -> &Arc<LedgerStorage> {
        &self.storage
    }

    /// Serves until ctrl-c, then shuts both listeners down.
    ///
    /// # Errors
    ///
    /// Returns the error the HTTP server stopped with.
    pub async fn serve(self) -> anyhow::Result<()> {
        self.serve_until(async {
            if let Err(error) = tokio::signal::ctrl_c().await {
                warn!(%error, "listening for ctrl-c failed; the node keeps serving");
                std::future::pending::<()>().await;
            }
        })
        .await
    }

    /// Serves until `shutdown` resolves, then shuts both listeners down.
    ///
    /// # Errors
    ///
    /// Returns the error the HTTP server stopped with.
    pub async fn serve_until<F>(self, shutdown: F) -> anyhow::Result<()>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let Self {
            listener,
            app,
            iroh,
            ..
        } = self;
        let served = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await;
        let endpoint = iroh.endpoint().clone();
        if let Err(error) = iroh.shutdown().await {
            warn!(%error, "the sync server did not shut down cleanly");
        }
        endpoint.close().await;
        info!("the node has stopped");
        served.map_err(anyhow::Error::from)
    }
}

/// Whether `node.json` carries a `role` key.
///
/// The typed config defaults the field, so the file itself is what says whether
/// an operator wrote it. Anything unreadable answers false: this decides one log
/// line and must not fail a start.
fn declares_role(home: &NodeHome) -> bool {
    std::fs::read(home.config_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|value| value.as_object().map(|object| object.contains_key("role")))
        .unwrap_or(false)
}
