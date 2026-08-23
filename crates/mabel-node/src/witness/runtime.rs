//! Starting and stopping a witness: the Iroh endpoint, the sync server and the
//! read-only HTTP API, over one home.
//!
//! [`WitnessRuntime::start`] does everything that can fail and reports where
//! the witness ended up listening, so a caller prints the endpoint id and both
//! addresses before the serve loop begins. [`WitnessRuntime::serve`] then runs
//! until ctrl-c and shuts both listeners down.

use std::future::Future;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;

use iroh::address_lookup::memory::MemoryLookup;
use iroh::endpoint::presets;
use iroh::protocol::Router as IrohRouter;
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode as IrohRelayMode};
use iroh_base::SecretKey;
use mabel_net::{ALPN, LedgerProtocol};
use tokio::net::TcpListener;
use tracing::{info, warn};

use crate::api::{ApiOptions, UiSource, witness_router};
use crate::config::{NodeConfig, NodeRole, RelayMode};
use crate::home::NodeHome;
use crate::witness::service::WitnessReadService;
use crate::witness::storage::{WitnessCaps, WitnessStorage};
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
        let storage = Arc::new(
            tokio::task::spawn_blocking(move || WitnessStorage::open(opened, endpoint_id, caps))
                .await??,
        );

        let endpoint = bind_endpoint(&config, secret, options.iroh_port, &options.peers).await?;
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
        let app = witness_router(service, &ApiOptions::new(bound.address).with_ui(options.ui));

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

/// Binds the Iroh endpoint per `node.json`, on `port` when one is asked for.
///
/// This mirrors [`mabel_net::bind_endpoint`], which takes no bind port: an
/// `--iroh-port` is what makes a witness reachable at a fixed address in the
/// compose topology.
async fn bind_endpoint(
    config: &NodeConfig,
    secret: SecretKey,
    port: Option<u16>,
    peers: &[EndpointAddr],
) -> anyhow::Result<Endpoint> {
    let lookup = MemoryLookup::new();
    for addr in peers {
        lookup.add_endpoint_info(addr.clone());
    }
    let mut builder = match config.relay {
        RelayMode::N0 => Endpoint::builder(presets::N0),
        RelayMode::Disabled => {
            Endpoint::builder(presets::Minimal).relay_mode(IrohRelayMode::Disabled)
        }
    };
    builder = builder.address_lookup(lookup).secret_key(secret);
    if let Some(port) = port {
        let address = SocketAddr::from((Ipv4Addr::UNSPECIFIED, port));
        builder = builder
            .clear_ip_transports()
            .bind_addr(address)
            .map_err(|error| anyhow::anyhow!("{address} is not a bindable address: {error}"))?;
    }
    builder
        .bind()
        .await
        .map_err(|error| anyhow::anyhow!("the Iroh endpoint could not bind: {error}"))
}
