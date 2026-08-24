//! The mabel app: one webview over a wallet node running in this process.
//!
//! [`node`] holds everything that starts and stops the node and depends on
//! nothing from tauri, so it compiles and its test runs on a machine with no
//! webview toolkit. [`app`] is the glue: it decides where the node home lives,
//! starts the node and opens the window on the URL the node serves. Everything
//! the window shows comes from the node over loopback HTTP, so the app ships no
//! frontend of its own beyond the page in `app/dist` that a failed start
//! displays.

pub mod node;

#[cfg(feature = "tauri-app")]
mod app;

#[cfg(feature = "tauri-app")]
pub use app::run;
