//! Runs the tauri build step only when the webview glue is compiled in.
//!
//! `tauri_build::build` reads `tauri.conf.json`, emits the `desktop` and
//! `mobile` cfg aliases and generates the context the binary embeds. With
//! `--no-default-features` there is no webview and nothing to generate.

fn main() {
    #[cfg(feature = "tauri-app")]
    tauri_build::build();
}
