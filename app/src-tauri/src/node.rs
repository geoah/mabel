//! The wallet node, running inside the app process.
//!
//! This is the same code path as `mabel wallet serve`: it opens or creates a
//! node home, binds the Iroh endpoint and the loopback HTTP listener through
//! [`WalletRuntime`], and serves the JSON API plus the UI bundle that
//! `mabel-node` compiled in from `ui/dist`. The app then points its webview at
//! [`RunningNode::wallet_url`].
//!
//! The listener always takes an ephemeral port on `127.0.0.1`, so two copies of
//! the app, or the app and a `mabel wallet serve` on the default port 9080,
//! never fight over one port. The API's loopback rules require `Host` to be
//! `127.0.0.1:<port>` or `localhost:<port>`, which a webview loading
//! `http://127.0.0.1:<port>/wallet` sends by itself.
//!
//! Nothing here mentions tauri, so it compiles and its test runs on a machine
//! with no webview toolkit installed.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};

use mabel_node::api::UiSource;
use mabel_node::wallet::{WalletOptions, WalletRuntime};
use mabel_node::{HomeOptions, NodeConfig, NodeHome, NodeRole, RelayMode};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tracing::{info, warn};

/// The node home inside the app's data directory.
///
/// Kept in a subdirectory so the app can write its own files beside a home
/// whose layout `mabel-node` owns.
pub const HOME_DIR_NAME: &str = "node";

/// The UI route the app opens, the wallet home of the React app.
pub const WALLET_ROUTE: &str = "/wallet";

/// What to start, and where.
#[derive(Debug, Clone)]
pub struct NodeOptions {
    /// The node home directory, created on first run.
    pub home: PathBuf,
    /// Whether the Iroh endpoint uses the n0 relays. Written into `node.json`
    /// the first time the home is created and read from it afterwards.
    pub relay: RelayMode,
    /// Where the UI bundle comes from. Embedded is the bundle `mabel-node`
    /// compiled in; a directory is for a person editing the UI.
    pub ui: UiSource,
}

impl NodeOptions {
    /// Options for a home at `home`.
    #[must_use]
    pub fn at(home: impl Into<PathBuf>) -> Self {
        Self {
            home: home.into(),
            relay: RelayMode::N0,
            ui: UiSource::Embedded,
        }
    }

    /// Options for the home the app keeps under its own data directory.
    #[must_use]
    pub fn under_data_dir(data_dir: &Path) -> Self {
        Self::at(data_dir.join(HOME_DIR_NAME))
    }

    /// The same options with the relays off, which is what a test wants: the
    /// endpoint then binds a UDP socket and reaches nothing.
    #[must_use]
    pub fn without_relays(mut self) -> Self {
        self.relay = RelayMode::Disabled;
        self
    }

    /// The same options serving the UI from a directory instead of the bundle
    /// compiled into `mabel-node`.
    #[must_use]
    pub fn with_ui_dir(mut self, directory: impl Into<PathBuf>) -> Self {
        self.ui = UiSource::Directory(directory.into());
        self
    }
}

/// A wallet node that is serving, and the handle that stops it.
#[derive(Debug)]
pub struct RunningNode {
    address: SocketAddr,
    endpoint_id: String,
    home: PathBuf,
    shutdown: Option<oneshot::Sender<()>>,
    served: JoinHandle<anyhow::Result<()>>,
}

impl RunningNode {
    /// Where the HTTP API listens, with the ephemeral port resolved.
    #[must_use]
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// This node's Iroh endpoint id, which a peer dials to fetch its ledgers.
    #[must_use]
    pub fn endpoint_id(&self) -> &str {
        &self.endpoint_id
    }

    /// The home this node serves.
    #[must_use]
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// The URL the webview loads.
    #[must_use]
    pub fn wallet_url(&self) -> String {
        format!("http://{}{WALLET_ROUTE}", self.address)
    }

    /// Asks the serve loop to shut down without waiting for it, which is what
    /// an exit handler that cannot await does.
    pub fn request_stop(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }

    /// Stops both listeners and waits for the serve loop to finish.
    ///
    /// # Errors
    ///
    /// Returns the error the HTTP server stopped with, or the panic of the
    /// serve task.
    pub async fn stop(mut self) -> anyhow::Result<()> {
        self.request_stop();
        (&mut self.served).await?
    }
}

impl Drop for RunningNode {
    fn drop(&mut self) {
        self.request_stop();
    }
}

/// Opens or creates the home and starts serving it as a wallet.
///
/// # Errors
///
/// Returns the errors of creating the home, of reading an insecure `node.key`,
/// and of binding either listener.
pub async fn start(options: NodeOptions) -> anyhow::Result<RunningNode> {
    let NodeOptions { home, relay, ui } = options;
    let root = home.clone();
    let home = tokio::task::spawn_blocking(move || {
        let config = NodeConfig {
            relay,
            ..NodeConfig::for_role(NodeRole::Wallet)
        };
        NodeHome::open_or_create(root, &config, HomeOptions::default())
    })
    .await??;
    let root = home.root().to_path_buf();

    let runtime = WalletRuntime::start(
        home,
        WalletOptions {
            http_bind: Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))),
            iroh_port: None,
            peers: Vec::new(),
            ui,
        },
    )
    .await?;

    let address = runtime.http_address();
    // The base32 spelling the API and the UI use, not the default rendering of
    // the key type, so a log line can be pasted into the UI's lookup field.
    let endpoint_id = mabel_node::wallet::ids::key(&runtime.endpoint_id()).to_string();
    if let Some(warning) = runtime.warning() {
        warn!(%warning, "the wallet node bound an address that is not loopback");
    }
    info!(%address, %endpoint_id, home = %root.display(), "the app's wallet node is serving");

    let (shutdown, stopping) = oneshot::channel();
    let served = tokio::spawn(async move {
        runtime
            .serve_until(async move {
                let _ = stopping.await;
            })
            .await
    });

    Ok(RunningNode {
        address,
        endpoint_id,
        home: root,
        shutdown: Some(shutdown),
        served,
    })
}

#[cfg(test)]
mod tests {
    use super::{NodeOptions, start};
    use std::net::SocketAddr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    /// One HTTP/1.1 GET, written by hand so the test needs no HTTP client.
    async fn get(address: SocketAddr, path: &str, host: &str) -> String {
        let mut stream = TcpStream::connect(address).await.expect("connect");
        let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
        stream.write_all(request.as_bytes()).await.expect("write");
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .expect("read the response");
        response
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_embedded_node_answers_the_node_route_on_an_ephemeral_port() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let node = start(NodeOptions::at(directory.path().join("node")).without_relays())
            .await
            .expect("the node starts");

        assert!(node.address().ip().is_loopback());
        assert_ne!(node.address().port(), 0, "the port is resolved");
        assert!(
            node.home().join("node.json").is_file(),
            "the home is created"
        );
        assert_eq!(
            node.wallet_url(),
            format!("http://{}/wallet", node.address())
        );

        let host = node.address().to_string();
        let answer = get(node.address(), "/api/node", &host).await;
        assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
        assert!(answer.contains("\"role\":\"wallet\""), "{answer}");
        assert!(answer.contains("storage_capacity"), "{answer}");
        assert!(
            answer.contains(node.endpoint_id()),
            "the answer names this endpoint: {answer}"
        );

        // The loopback rules the webview satisfies and a rebinding attacker
        // does not.
        let refused = get(node.address(), "/api/node", "evil.example").await;
        assert!(refused.starts_with("HTTP/1.1 403"), "{refused}");
        assert!(refused.contains("host_not_loopback"), "{refused}");

        node.stop().await.expect("the node stops");
    }
}
