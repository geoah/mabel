//! Starting and stopping a witness: the Iroh endpoint, the sync server and the
//! read-only HTTP API, over one home.
//!
//! [`WitnessRuntime::start`] does everything that can fail and reports where
//! the witness ended up listening, so a caller prints the endpoint id and both
//! addresses before the serve loop begins. [`WitnessRuntime::serve`] then runs
//! until ctrl-c and shuts both listeners down.

use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;

use iroh::protocol::Router as IrohRouter;
use iroh::{EndpointAddr, EndpointId};
use mabel_net::{ALPN, LedgerProtocol};
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::api::{ApiOptions, UiSource, witness_router};
use crate::config::NodeRole;
use crate::endpoint::bind_endpoint;
use crate::home::NodeHome;
use crate::witness::service::WitnessReadService;
use crate::witness::storage::{AdmissionPolicy, WitnessCaps, WitnessStorage};
use crate::witness::store::WitnessStore;

/// What `mabel witness run` was told on the command line.
#[derive(Debug, Default)]
pub struct WitnessOptions {
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
    /// The caps to enforce, `node.json`'s and section 5's unless a test
    /// shrinks them.
    pub caps: Option<WitnessCaps>,
}

/// A witness that has bound both listeners and not yet begun serving.
#[derive(Debug)]
pub struct WitnessRuntime {
    endpoint_id: EndpointId,
    http_address: SocketAddr,
    iroh_addresses: Vec<SocketAddr>,
    warning: Option<String>,
    allowed_hosts: Vec<String>,
    listener: TcpListener,
    app: axum::Router,
    iroh: IrohRouter,
    storage: Arc<WitnessStorage>,
}

impl WitnessRuntime {
    /// Reads the home, rebuilds the folded state, binds the Iroh endpoint and
    /// the HTTP listener.
    ///
    /// # Errors
    ///
    /// Returns the errors of reading `node.json` and `node.key`, of rebuilding
    /// the index from the event files, and of binding either listener.
    pub async fn start(home: NodeHome, options: WitnessOptions) -> anyhow::Result<Self> {
        let config = home.config()?;
        if config.role != NodeRole::Witness {
            warn!(
                "node.json says this home is a {:?} home; running it as a witness stores no keys \
                 and signs nothing",
                config.role
            );
        }
        let secret = home.node_key()?;
        let endpoint_id = secret.public();

        let caps = options
            .caps
            .unwrap_or_else(|| WitnessCaps::from_config(&config));
        let opened = home.clone();
        // Which identities this home witnesses for, and whether the retired
        // tag-11 clause may admit a push, are read once at startup: an operator
        // who edits either restarts the node, exactly as they do for every
        // other `node.json` value (proposal 006 section 4).
        let policy = AdmissionPolicy::from_config(&config);
        let storage = Arc::new(
            tokio::task::spawn_blocking(move || {
                WitnessStorage::open(opened, endpoint_id, caps, policy)
            })
            .await??,
        );

        let endpoint =
            bind_endpoint(config.relay, secret, options.iroh_port, &options.peers).await?;
        let iroh_addresses = endpoint.bound_sockets();
        let store = Arc::new(WitnessStore::new(storage.clone()));
        let iroh = IrohRouter::builder(endpoint)
            .accept(ALPN, LedgerProtocol::new(store))
            .spawn();

        let bound = crate::api::bind::bind(options.http_bind.unwrap_or(config.http_bind)).await?;
        let service = Arc::new(WitnessReadService::new(
            storage.clone(),
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
        let app = witness_router(service, &api);

        info!(
            %endpoint_id,
            http = %bound.address,
            "the witness is listening"
        );
        Ok(Self {
            endpoint_id,
            http_address: bound.address,
            iroh_addresses,
            warning: bound.warning,
            allowed_hosts,
            listener: bound.listener,
            app,
            iroh,
            storage,
        })
    }

    /// This witness's Iroh endpoint id, which a wallet names in a
    /// `WitnessConfig`.
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

    /// The hosts this witness accepts beyond loopback, `node.json` and
    /// `--allow-host` merged (decision 018).
    #[must_use]
    pub fn allowed_hosts(&self) -> &[String] {
        &self.allowed_hosts
    }

    /// The storage both surfaces share.
    #[must_use]
    pub fn storage(&self) -> &Arc<WitnessStorage> {
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
                warn!(%error, "listening for ctrl-c failed; the witness keeps serving");
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
        info!("the witness has stopped");
        served.map_err(anyhow::Error::from)
    }
}
