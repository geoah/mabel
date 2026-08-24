//! The webview glue: start the node, then open the window on it.
//!
//! The window is created after the node is listening and is created directly on
//! `http://127.0.0.1:<port>/wallet`, so the app never navigates across origins
//! and the page the user sees is the one the node serves. Starting the node
//! blocks the setup hook, which is a bind of two sockets and a directory read.
//!
//! When the node cannot start, the window opens on the page in `app/dist`
//! instead and the reason goes to the log. Nothing here registers an IPC
//! command: the UI talks to the node over HTTP exactly as it does in a browser,
//! and the page has no capability to reach tauri.

use std::sync::Mutex;

use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tracing::{error, info};

use crate::node::{self, NodeOptions, RunningNode};

/// The window label, and the only window this app opens.
const MAIN_WINDOW: &str = "main";

/// The running node, kept in tauri's state so the exit handler can stop it.
struct NodeState(Mutex<Option<RunningNode>>);

/// Runs the app until the user closes it.
///
/// # Panics
///
/// Panics when tauri cannot build the app, which means the bundled
/// configuration or the webview toolkit is broken and there is nothing to show.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let app = tauri::Builder::default()
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = handle.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            info!(data_dir = %data_dir.display(), "the app data directory");

            let started =
                tauri::async_runtime::block_on(node::start(NodeOptions::under_data_dir(&data_dir)));
            let url = match started {
                Ok(node) => {
                    let url = tauri::Url::parse(&node.wallet_url())?;
                    handle.manage(NodeState(Mutex::new(Some(node))));
                    WebviewUrl::External(url)
                }
                Err(error) => {
                    error!(%error, "the wallet node did not start; opening the failure page");
                    WebviewUrl::App("index.html".into())
                }
            };

            WebviewWindowBuilder::new(&handle, MAIN_WINDOW, url)
                .title("Mabel")
                .inner_size(1100.0, 800.0)
                .min_inner_size(420.0, 560.0)
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("the tauri app builds");

    app.run(|handle, event| {
        if matches!(event, RunEvent::Exit) {
            stop_node(handle);
        }
    });
}

/// Shuts the node down on exit, so the Iroh endpoint closes and the home is
/// left with no half-written file.
fn stop_node<R: tauri::Runtime>(handle: &tauri::AppHandle<R>) {
    let Some(state) = handle.try_state::<NodeState>() else {
        return;
    };
    let node = state.0.lock().ok().and_then(|mut held| held.take());
    if let Some(node) = node {
        if let Err(error) = tauri::async_runtime::block_on(node.stop()) {
            error!(%error, "the wallet node did not stop cleanly");
        } else {
            info!("the wallet node stopped");
        }
    }
}
