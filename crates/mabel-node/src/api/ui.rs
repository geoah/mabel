//! Static UI assets, embedded or from disk (proposal 001 section 10).
//!
//! The bundle in `ui/dist` is embedded with `rust-embed` when it exists at
//! compile time and `--ui-dir` overrides it with a directory read at runtime.
//! The embed is declared `allow_missing`, so a checkout whose `ui/dist` has
//! never been built still compiles: the binary then serves nothing and says so
//! (`ui_not_built`) instead of failing the build.
//!
//! One app serves both routes, so any path that is not a file falls back to
//! `index.html` and the router in the browser decides what to draw.

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

use super::error::ServiceError;

/// The compiled-in bundle, empty when `ui/dist` did not exist at compile time.
#[derive(RustEmbed)]
#[folder = "../../ui/dist"]
#[allow_missing = true]
struct EmbeddedUi;

/// The file `serve` falls back to for any path that is not an asset.
const INDEX: &str = "index.html";

/// Where the UI bundle comes from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum UiSource {
    /// The bundle compiled into this binary.
    #[default]
    Embedded,
    /// A directory read at runtime, from `--ui-dir`.
    Directory(PathBuf),
    /// Serve the JSON API only.
    Disabled,
}

impl UiSource {
    /// `--ui-dir <path>`, or the embedded bundle when the flag is absent.
    #[must_use]
    pub fn from_option(directory: Option<PathBuf>) -> Self {
        directory.map_or(Self::Embedded, Self::Directory)
    }
}

/// Serves one request path from the UI bundle.
///
/// # Errors
///
/// Returns a 404 envelope: `ui_disabled` when this node serves no UI,
/// `ui_not_built` when the bundle is empty, and `ui_asset_not_found` for a
/// path that names a file the bundle does not hold.
pub(super) async fn serve(source: &UiSource, request_path: &str) -> Result<Response, ServiceError> {
    if *source == UiSource::Disabled {
        return Err(
            ServiceError::usage("ui_disabled", "this node serves the JSON API only")
                .with_status(StatusCode::NOT_FOUND),
        );
    }
    let Some(relative) = normalize(request_path) else {
        return Err(not_found("ui_asset_not_found", request_path));
    };
    if let Some(bytes) = load(source, &relative).await {
        return Ok(asset(&relative, bytes));
    }
    if relative == INDEX {
        return Err(not_built());
    }
    if names_a_file(&relative) {
        return Err(not_found("ui_asset_not_found", request_path));
    }
    match load(source, INDEX).await {
        Some(bytes) => Ok(asset(INDEX, bytes)),
        None => Err(not_built()),
    }
}

/// One asset, from wherever this node keeps them.
async fn load(source: &UiSource, relative: &str) -> Option<Vec<u8>> {
    match source {
        UiSource::Disabled => None,
        UiSource::Embedded => EmbeddedUi::get(relative).map(|file| file.data.into_owned()),
        UiSource::Directory(directory) => read(directory, relative).await,
    }
}

/// A request path as a relative asset path, or `None` when it tries to leave
/// the bundle.
fn normalize(request_path: &str) -> Option<String> {
    let trimmed = request_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some(INDEX.to_owned());
    }
    if trimmed.contains('\\') || trimmed.split('/').any(|segment| segment == "..") {
        return None;
    }
    Some(trimmed.to_owned())
}

/// Whether the path names a file rather than a route of the browser app. A
/// route has no extension in its last segment.
fn names_a_file(relative: &str) -> bool {
    relative
        .rsplit('/')
        .next()
        .is_some_and(|segment| segment.contains('.'))
}

async fn read(directory: &Path, relative: &str) -> Option<Vec<u8>> {
    tokio::fs::read(directory.join(relative)).await.ok()
}

fn asset(relative: &str, bytes: Vec<u8>) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, content_type(relative))],
        Body::from(bytes),
    )
        .into_response()
}

fn not_found(reason: &str, request_path: &str) -> ServiceError {
    ServiceError::usage(reason, format!("no asset {request_path} in the UI bundle"))
        .with_detail("path", request_path)
        .with_status(StatusCode::NOT_FOUND)
}

fn not_built() -> ServiceError {
    ServiceError::usage(
        "ui_not_built",
        "no UI bundle is compiled into this binary; build ui/dist or pass --ui-dir",
    )
    .with_status(StatusCode::NOT_FOUND)
}

/// The content type for an extension, since the node depends on no MIME
/// database.
fn content_type(relative: &str) -> &'static str {
    let extension = relative
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::{UiSource, content_type, names_a_file, normalize, serve};
    use axum::body::to_bytes;
    use axum::http::{StatusCode, header};

    async fn body(response: axum::response::Response) -> String {
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn bundle() -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("a temp dir");
        std::fs::write(directory.path().join("index.html"), "<!doctype html>").unwrap();
        std::fs::write(directory.path().join("app.js"), "export {};").unwrap();
        directory
    }

    #[tokio::test]
    async fn a_directory_bundle_serves_files_with_their_content_type() {
        let directory = bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());

        let response = serve(&source, "/app.js").await.expect("app.js");
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(body(response).await, "export {};");

        let response = serve(&source, "/").await.expect("index");
        assert_eq!(body(response).await, "<!doctype html>");
    }

    #[tokio::test]
    async fn an_app_route_falls_back_to_index_and_a_missing_file_does_not() {
        let directory = bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());

        let response = serve(&source, "/witness").await.expect("an app route");
        assert_eq!(body(response).await, "<!doctype html>");

        let error = serve(&source, "/missing.js")
            .await
            .expect_err("no such file");
        assert_eq!(error.status(), StatusCode::NOT_FOUND);
        assert_eq!(error.reason(), "ui_asset_not_found");
    }

    #[tokio::test]
    async fn a_path_that_climbs_out_of_the_bundle_is_refused() {
        let directory = bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());
        for path in ["/../secret.key", "/assets/../../secret.key"] {
            let error = serve(&source, path).await.expect_err(path);
            assert_eq!(error.reason(), "ui_asset_not_found", "{path}");
        }
        assert!(normalize("/../etc/passwd").is_none());
    }

    #[tokio::test]
    async fn an_empty_bundle_says_the_ui_is_not_built() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let source = UiSource::Directory(directory.path().to_path_buf());
        let error = serve(&source, "/").await.expect_err("nothing to serve");
        assert_eq!(error.reason(), "ui_not_built");
        assert_eq!(error.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_disabled_ui_answers_404() {
        let error = serve(&UiSource::Disabled, "/").await.expect_err("no ui");
        assert_eq!(error.reason(), "ui_disabled");
    }

    #[test]
    fn ui_dir_overrides_the_embed_only_when_it_is_given() {
        assert_eq!(UiSource::from_option(None), UiSource::Embedded);
        assert_eq!(
            UiSource::from_option(Some("/tmp/dist".into())),
            UiSource::Directory("/tmp/dist".into())
        );
    }

    #[test]
    fn a_route_is_told_from_a_file_by_its_extension() {
        assert!(names_a_file("assets/app-4f3a.js"));
        assert!(!names_a_file("witness"));
        assert!(!names_a_file("identities/alice"));
        assert_eq!(content_type("x.woff2"), "font/woff2");
        assert_eq!(content_type("x.unknown"), "application/octet-stream");
    }
}
