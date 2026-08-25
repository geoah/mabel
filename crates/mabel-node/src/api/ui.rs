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
//!
//! # Caching
//!
//! Vite writes a content hash into the filenames it emits, so those bytes
//! never change under their name and are served `public, max-age=31536000,
//! immutable`. [`is_content_hashed`] is what decides that, by reading the
//! name: a directory handed to `--ui-dir` may hold an unhashed
//! `assets/logo.svg`, and the path alone would have cached it for a year.
//! Everything else, `index.html` included, is served `no-cache`: the browser
//! revalidates on every load, and an [`ETag`] turns that revalidation into a
//! 304 with no body. That pair is what survives an upgrade. Without it a
//! browser holding a cached `index.html` asks the new binary for the old
//! hashed asset, which is gone, and the app is broken until someone
//! hard-refreshes (issue 043).
//!
//! # Compression
//!
//! `ui/precompress.ts` writes a `.br` and a `.gz` beside every compressible
//! file at build time, and they are embedded with the rest of the bundle.
//! [`serve`] picks one from `Accept-Encoding` and sends the stored bytes: no
//! request compresses anything, and a client that offers no encoding gets the
//! original file. Every asset answer carries `Vary: Accept-Encoding`, since
//! one URL now has up to three representations, and each representation's
//! `ETag` is the hash of the bytes that representation carries.
//!
//! No response-compression layer runs over these routes. One would re-encode
//! a file that had no stored sibling and attach this module's validator to
//! bytes it never saw, so `tower_http::CompressionLayer` is scoped to the JSON
//! routes alone (`super::node_router`).
//!
//! [`ETag`]: https://www.rfc-editor.org/rfc/rfc9110#field.etag

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;
use sha2::{Digest, Sha256};

use super::error::ServiceError;

/// The compiled-in bundle, empty when `ui/dist` did not exist at compile time.
#[derive(RustEmbed)]
#[folder = "../../ui/dist"]
#[allow_missing = true]
struct EmbeddedUi;

/// The file `serve` falls back to for any path that is not an asset.
const INDEX: &str = "index.html";

/// What a hashed asset is cached as: a year, and never revalidated. The hash
/// in the name is the version, so a changed file is a changed URL.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// What everything else is cached as: keep it, but ask before using it. The
/// `ETag` makes that question cheap.
const REVALIDATE: &str = "no-cache";

/// One stored representation of one asset.
struct Representation {
    /// The bytes to send.
    bytes: Vec<u8>,
    /// The sha256 of `bytes`, lowercase hex. Of these bytes, not of the file
    /// they were compressed from: it is this answer's validator.
    hash: String,
    /// The `Content-Encoding` they are in, absent for the original file.
    encoding: Option<&'static str>,
}

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
/// `request_headers` decides two things and nothing else: which stored
/// representation `Accept-Encoding` allows, and whether `If-None-Match` lets
/// this answer be a 304. Nothing here reads `Host`, so a reverse proxy in
/// front of this node changes no answer.
///
/// # Errors
///
/// Returns a 404 envelope: `ui_disabled` when this node serves no UI,
/// `ui_not_built` when the bundle is empty, and `ui_asset_not_found` for a
/// path that names a file the bundle does not hold.
pub(super) async fn serve(
    source: &UiSource,
    request_path: &str,
    request_headers: &HeaderMap,
) -> Result<Response, ServiceError> {
    if *source == UiSource::Disabled {
        return Err(
            ServiceError::usage("ui_disabled", "this node serves the JSON API only")
                .with_status(StatusCode::NOT_FOUND),
        );
    }
    let Some(relative) = normalize(request_path) else {
        return Err(not_found("ui_asset_not_found", request_path));
    };
    if let Some(answer) = answer(source, &relative, request_headers).await {
        return Ok(answer);
    }
    if relative == INDEX {
        return Err(not_built());
    }
    if names_a_file(&relative) {
        return Err(not_found("ui_asset_not_found", request_path));
    }
    match answer(source, INDEX, request_headers).await {
        Some(response) => Ok(response),
        None => Err(not_built()),
    }
}

/// The response for one asset the bundle holds, or `None` when it holds none.
async fn answer(
    source: &UiSource,
    relative: &str,
    request_headers: &HeaderMap,
) -> Option<Response> {
    let representation = negotiate(source, relative, request_headers).await?;
    // The validator is the hash of the bytes this answer carries, not of the
    // file they were compressed from. A `.br` sibling rebuilt from the same
    // source is different bytes and gets a different tag, so a rebuilt
    // `--ui-dir` cannot 304 a client onto stale content.
    let etag = format!("\"{}\"", representation.hash);
    let cache_control = if is_content_hashed(relative) {
        IMMUTABLE
    } else {
        REVALIDATE
    };
    if matches_etag(request_headers, &etag) {
        return Some(not_modified(&etag, cache_control));
    }
    Some(asset(relative, &representation, &etag, cache_control))
}

/// Whether the filename carries a build-time content hash, which is the only
/// thing that makes `immutable` true.
///
/// Vite writes `<name>-<hash>.<ext>`, and the hash is base64url over a content
/// digest: `index-BM2eU1h0.js`. Marking anything else immutable caches it for a
/// year under a name that can change, so this errs the safe way. A hash this
/// misses costs one revalidation; a name this wrongly accepts costs a year of
/// a browser refusing to look (issue 043).
fn is_content_hashed(relative: &str) -> bool {
    // The stem is everything before the first dot, so `app-BM2eU1h0.js` and
    // its `app-BM2eU1h0.js.map` are read the same way.
    let name = relative.rsplit('/').next().unwrap_or_default();
    let stem = name.split('.').next().unwrap_or_default();
    let Some((prefix, tail)) = stem.rsplit_once('-') else {
        return false;
    };
    if prefix.is_empty() || !(8..=32).contains(&tail.len()) {
        return false;
    }
    if !tail
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return false;
    }
    // A digest over a 64-character alphabet has all but certainly got a digit
    // or a capital in it; an English word in a hand-built directory has
    // neither, and falls back to revalidating.
    tail.bytes()
        .any(|byte| byte.is_ascii_digit() || byte.is_ascii_uppercase())
}

/// Whether `If-None-Match` names this exact representation.
///
/// A weak comparison, which is what RFC 9110 asks of `If-None-Match`: `W/` in
/// front of a tag makes no difference to whether it matches. Every field line
/// is read, since a client may send the tags it holds across several.
fn matches_etag(request_headers: &HeaderMap, etag: &str) -> bool {
    request_headers
        .get_all(header::IF_NONE_MATCH)
        .into_iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|line| line.split(','))
        .any(|candidate| {
            let candidate = candidate.trim();
            candidate == "*" || candidate.trim_start_matches("W/") == etag
        })
}

/// A 304, which carries the validator and the caching rule and no body.
fn not_modified(etag: &str, cache_control: &'static str) -> Response {
    let mut response = StatusCode::NOT_MODIFIED.into_response();
    set(&mut response, header::ETAG, etag);
    set(&mut response, header::CACHE_CONTROL, cache_control);
    set(&mut response, header::VARY, "Accept-Encoding");
    response
}

/// Sets one header, dropping a value that cannot be one. Every value here is
/// hex, a fixed string or a stored encoding name, so none can fail.
fn set(response: &mut Response, name: header::HeaderName, value: &str) {
    if let Ok(value) = HeaderValue::from_str(value) {
        response.headers_mut().insert(name, value);
    }
}

/// One stored file, from wherever this node keeps them, with the hash of the
/// bytes it holds.
///
/// The embedded bundle carries a sha256 per file computed at compile time, so
/// the common path hashes nothing per request, and it carries one per sibling
/// too: the `.br` file is a file. A directory bundle is a developer's
/// `--ui-dir` and is hashed on read.
async fn load(source: &UiSource, relative: &str) -> Option<Representation> {
    match source {
        UiSource::Disabled => None,
        UiSource::Embedded => EmbeddedUi::get(relative).map(|file| Representation {
            hash: hex(&file.metadata.sha256_hash()),
            bytes: file.data.into_owned(),
            encoding: None,
        }),
        UiSource::Directory(directory) => {
            read(directory, relative).await.map(|bytes| Representation {
                hash: hex(&Sha256::digest(&bytes)),
                bytes,
                encoding: None,
            })
        }
    }
}

/// The stored representation this request may have, or `None` when the bundle
/// holds no such asset.
///
/// The acceptable coding with the highest quality wins, brotli ahead of gzip on
/// a tie. A missing sibling is not an error: it means that file did not
/// compress, or the bundle predates `ui/precompress.ts`, and the original is
/// always correct. Nothing is ever compressed here, so an encoding with no
/// stored sibling simply falls through to the next candidate.
async fn negotiate(
    source: &UiSource,
    relative: &str,
    request_headers: &HeaderMap,
) -> Option<Representation> {
    let accepted = request_headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let mut candidates = [
        ("br", ".br", quality(accepted, "br")),
        ("gzip", ".gz", quality(accepted, "gzip")),
    ];
    // Descending quality, and the array order breaks a tie, so `gzip, br` with
    // no q values still prefers brotli.
    candidates.sort_by(|left, right| right.2.total_cmp(&left.2));
    for (encoding, extension, q) in candidates {
        if q <= 0.0 {
            continue;
        }
        if let Some(sibling) = load(source, &format!("{relative}{extension}")).await {
            return Some(Representation {
                encoding: Some(encoding),
                ..sibling
            });
        }
    }
    // Every coding refused, `identity;q=0` included: RFC 9110 allows a 406
    // here, and this answers with the file instead. A wallet that will not
    // load is a worse failure than one that ignores an exotic header, and
    // nothing in this bundle is unreadable as it is stored.
    load(source, relative).await
}

/// The quality `header` gives `token`, 0 when it refuses it.
///
/// An explicit entry wins over `*`, so `*;q=1, br;q=0` refuses brotli and
/// still allows gzip. A named coding with no `q` is 1, and a `q` that does not
/// parse is 1: a malformed parameter is not a refusal.
fn quality(header: &str, token: &str) -> f32 {
    let mut wildcard = None;
    for part in header.split(',') {
        let mut fields = part.split(';');
        let name = fields.next().unwrap_or_default().trim();
        if name.is_empty() {
            continue;
        }
        let q = fields
            .filter_map(|field| {
                let field = field.trim();
                let (key, value) = field.split_once('=')?;
                key.trim().eq_ignore_ascii_case("q").then_some(value.trim())
            })
            .next()
            .map_or(1.0, |value| value.parse::<f32>().unwrap_or(1.0));
        if name.eq_ignore_ascii_case(token) {
            return q;
        }
        if name == "*" {
            wildcard = Some(q);
        }
    }
    wildcard.unwrap_or(0.0)
}

/// Lowercase hex, which is what an `ETag` for these bytes is spelled as.
fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
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

/// A 200 for one representation.
///
/// `Content-Type` is the type of the asset, never of the container it was
/// compressed into: a brotli-encoded stylesheet is still `text/css`.
fn asset(
    relative: &str,
    representation: &Representation,
    etag: &str,
    cache_control: &'static str,
) -> Response {
    let mut response = (
        StatusCode::OK,
        [(header::CONTENT_TYPE, content_type(relative))],
        Body::from(representation.bytes.clone()),
    )
        .into_response();
    set(&mut response, header::ETAG, etag);
    set(&mut response, header::CACHE_CONTROL, cache_control);
    set(&mut response, header::VARY, "Accept-Encoding");
    if let Some(encoding) = representation.encoding {
        set(&mut response, header::CONTENT_ENCODING, encoding);
    }
    response
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
    use super::{
        UiSource, content_type, hex, is_content_hashed, names_a_file, normalize, quality, serve,
    };
    use axum::body::to_bytes;
    use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
    use sha2::{Digest, Sha256};

    async fn body(response: axum::response::Response) -> String {
        let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    async fn raw(response: axum::response::Response) -> Vec<u8> {
        to_bytes(response.into_body(), 1 << 20)
            .await
            .unwrap()
            .to_vec()
    }

    /// A request offering nothing: no encoding, no validator.
    fn plain() -> HeaderMap {
        HeaderMap::new()
    }

    fn with(name: header::HeaderName, value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(name, HeaderValue::from_str(value).unwrap());
        headers
    }

    /// The `ETag` these bytes must carry: quoted sha256, lowercase hex.
    fn sha256_etag(bytes: &[u8]) -> String {
        format!("\"{}\"", hex(&Sha256::digest(bytes)))
    }

    fn header_of(response: &axum::response::Response, name: header::HeaderName) -> String {
        response
            .headers()
            .get(name)
            .map(|value| value.to_str().unwrap().to_owned())
            .unwrap_or_default()
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

        let response = serve(&source, "/app.js", &plain()).await.expect("app.js");
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(body(response).await, "export {};");

        let response = serve(&source, "/", &plain()).await.expect("index");
        assert_eq!(body(response).await, "<!doctype html>");
    }

    #[tokio::test]
    async fn an_app_route_falls_back_to_index_and_a_missing_file_does_not() {
        let directory = bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());

        let response = serve(&source, "/witness", &plain())
            .await
            .expect("an app route");
        assert_eq!(body(response).await, "<!doctype html>");

        let error = serve(&source, "/missing.js", &plain())
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
            let error = serve(&source, path, &plain()).await.expect_err(path);
            assert_eq!(error.reason(), "ui_asset_not_found", "{path}");
        }
        assert!(normalize("/../etc/passwd").is_none());
    }

    #[tokio::test]
    async fn an_empty_bundle_says_the_ui_is_not_built() {
        let directory = tempfile::tempdir().expect("a temp dir");
        let source = UiSource::Directory(directory.path().to_path_buf());
        let error = serve(&source, "/", &plain())
            .await
            .expect_err("nothing to serve");
        assert_eq!(error.reason(), "ui_not_built");
        assert_eq!(error.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn a_disabled_ui_answers_404() {
        let error = serve(&UiSource::Disabled, "/", &plain())
            .await
            .expect_err("no ui");
        assert_eq!(error.reason(), "ui_disabled");
    }

    /// A bundle shaped like a real one: a content-hashed asset with both
    /// precompressed siblings, an unhashed one beside it, and the html.
    fn compressed_bundle() -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("a temp dir");
        let assets = directory.path().join("assets");
        std::fs::create_dir(&assets).unwrap();
        std::fs::write(directory.path().join("index.html"), "<!doctype html>").unwrap();
        std::fs::write(assets.join(HASHED_NAME), "export const original = 1;").unwrap();
        std::fs::write(
            format!("{}.br", assets.join(HASHED_NAME).display()),
            b"brotli bytes",
        )
        .unwrap();
        std::fs::write(
            format!("{}.gz", assets.join(HASHED_NAME).display()),
            b"gzip bytes",
        )
        .unwrap();
        // A hand-assembled `--ui-dir` can hold a name with no hash in it.
        std::fs::write(assets.join("logo.svg"), "<svg/>").unwrap();
        directory
    }

    /// A Vite filename: the stem ends in a base64url digest.
    const HASHED_NAME: &str = "index-BM2eU1h0.js";
    const HASHED_PATH: &str = "/assets/index-BM2eU1h0.js";

    /// A content-hashed filename never names different bytes, so it is cached
    /// for a year and never revalidated (issue 043).
    #[tokio::test]
    async fn a_hashed_asset_is_immutable_for_a_year() {
        let directory = compressed_bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());

        let response = serve(&source, HASHED_PATH, &plain())
            .await
            .expect("the asset");
        assert_eq!(
            header_of(&response, header::CACHE_CONTROL),
            "public, max-age=31536000, immutable"
        );
        assert_eq!(header_of(&response, header::VARY), "Accept-Encoding");
        assert!(!header_of(&response, header::ETAG).is_empty());
    }

    /// The html is what names the hashed assets, so a browser must ask before
    /// reusing it. The `ETag` makes that question a 304.
    #[tokio::test]
    async fn the_html_and_every_app_route_revalidate_with_an_etag() {
        let directory = compressed_bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());

        let mut tags = Vec::new();
        for path in ["/", "/wallet", "/witnesses", "/identities/alice"] {
            let response = serve(&source, path, &plain()).await.expect(path);
            assert_eq!(
                header_of(&response, header::CACHE_CONTROL),
                "no-cache",
                "{path}"
            );
            let etag = header_of(&response, header::ETAG);
            assert!(
                etag.starts_with('"') && etag.ends_with('"'),
                "{path}: {etag}"
            );
            tags.push(etag);
        }
        // One file, one validator, whichever route served it.
        assert!(tags.windows(2).all(|pair| pair[0] == pair[1]), "{tags:?}");
    }

    /// The round trip that keeps a repeat load off the wire: read the `ETag`,
    /// send it back, get a 304 with no body.
    #[tokio::test]
    async fn an_if_none_match_round_trip_answers_304_with_no_body() {
        let directory = compressed_bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());

        let first = serve(&source, "/wallet", &plain()).await.expect("the html");
        assert_eq!(first.status(), StatusCode::OK);
        let etag = header_of(&first, header::ETAG);

        for offered in [
            etag.clone(),
            format!("W/{etag}"),
            format!("\"other\", {etag}"),
        ] {
            let response = serve(&source, "/wallet", &with(header::IF_NONE_MATCH, &offered))
                .await
                .expect("the html");
            assert_eq!(response.status(), StatusCode::NOT_MODIFIED, "{offered}");
            assert_eq!(header_of(&response, header::ETAG), etag, "{offered}");
            assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-cache");
            assert_eq!(header_of(&response, header::VARY), "Accept-Encoding");
            assert!(raw(response).await.is_empty(), "{offered}");
        }

        // A validator for other bytes is not a match.
        let response = serve(
            &source,
            "/wallet",
            &with(header::IF_NONE_MATCH, "\"stale\""),
        )
        .await
        .expect("the html");
        assert_eq!(response.status(), StatusCode::OK);
    }

    /// Brotli when the client takes it, gzip when it only takes that, and the
    /// file itself when it offers nothing (issue 043).
    #[tokio::test]
    async fn the_stored_representation_follows_accept_encoding() {
        let directory = compressed_bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());
        let path = HASHED_PATH;

        for (accept, encoding, bytes) in [
            ("br, gzip", "br", "brotli bytes"),
            ("gzip, deflate", "gzip", "gzip bytes"),
            ("gzip;q=0, br", "br", "brotli bytes"),
            ("*", "br", "brotli bytes"),
        ] {
            let response = serve(&source, path, &with(header::ACCEPT_ENCODING, accept))
                .await
                .expect(path);
            assert_eq!(
                header_of(&response, header::CONTENT_ENCODING),
                encoding,
                "{accept}"
            );
            // The type is the asset's, never the container's.
            assert_eq!(
                header_of(&response, header::CONTENT_TYPE),
                "text/javascript; charset=utf-8"
            );
            assert_eq!(header_of(&response, header::VARY), "Accept-Encoding");
            assert_eq!(body(response).await, bytes, "{accept}");
        }

        // No `Accept-Encoding` at all: the original bytes, no encoding named.
        let response = serve(&source, path, &plain()).await.expect(path);
        assert_eq!(header_of(&response, header::CONTENT_ENCODING), "");
        assert_eq!(body(response).await, "export const original = 1;");

        // A client asking for something nobody precompressed also gets the
        // original, rather than an encoding the bundle does not hold.
        let response = serve(&source, path, &with(header::ACCEPT_ENCODING, "zstd"))
            .await
            .expect(path);
        assert_eq!(header_of(&response, header::CONTENT_ENCODING), "");
        assert_eq!(body(response).await, "export const original = 1;");
    }

    /// One `ETag` per representation, and each is the hash of the bytes that
    /// representation carries. A tag over the original file would let a
    /// rebuilt `.br` sibling 304 a client onto bytes it does not hold.
    #[tokio::test]
    async fn every_representation_is_validated_by_its_own_bytes() {
        let directory = compressed_bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());
        let path = HASHED_PATH;

        let mut tags = Vec::new();
        for accept in ["br", "gzip", ""] {
            let response = serve(&source, path, &with(header::ACCEPT_ENCODING, accept))
                .await
                .expect(path);
            tags.push(header_of(&response, header::ETAG));
        }
        assert_eq!(tags[0], sha256_etag(b"brotli bytes"));
        assert_eq!(tags[1], sha256_etag(b"gzip bytes"));
        assert_eq!(tags[2], sha256_etag(b"export const original = 1;"));
        assert!(tags[0] != tags[1] && tags[1] != tags[2] && tags[0] != tags[2]);

        // The brotli validator does not satisfy a request for the plain bytes.
        let mut headers = with(header::IF_NONE_MATCH, &tags[0]);
        headers.insert(
            header::ACCEPT_ENCODING,
            HeaderValue::from_static("identity"),
        );
        let response = serve(&source, path, &headers).await.expect(path);
        assert_eq!(response.status(), StatusCode::OK);

        // Rewriting the sibling alone changes that representation's tag, so a
        // client holding the old one revalidates onto the new bytes instead of
        // being told nothing changed.
        std::fs::write(
            directory.path().join("assets").join("index-BM2eU1h0.js.br"),
            b"rebuilt brotli",
        )
        .unwrap();
        let mut headers = with(header::IF_NONE_MATCH, &tags[0]);
        headers.insert(header::ACCEPT_ENCODING, HeaderValue::from_static("br"));
        let response = serve(&source, path, &headers).await.expect(path);
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "a rebuilt sibling must not 304 against the old tag"
        );
        assert_eq!(
            header_of(&response, header::ETAG),
            sha256_etag(b"rebuilt brotli")
        );
    }

    /// `immutable` follows the filename, not the directory: only a name
    /// carrying a build hash gets a year (issue 043).
    #[tokio::test]
    async fn only_a_content_hashed_name_is_immutable() {
        let directory = compressed_bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());

        let response = serve(&source, HASHED_PATH, &plain()).await.expect("hashed");
        assert_eq!(
            header_of(&response, header::CACHE_CONTROL),
            "public, max-age=31536000, immutable"
        );

        // Same directory, no hash in the name: it revalidates, with a tag.
        let response = serve(&source, "/assets/logo.svg", &plain())
            .await
            .expect("logo");
        assert_eq!(header_of(&response, header::CACHE_CONTROL), "no-cache");
        assert!(!header_of(&response, header::ETAG).is_empty());
    }

    #[test]
    fn a_build_hash_is_told_from_a_hand_written_name() {
        for hashed in [
            "assets/index-BM2eU1h0.js",
            "assets/index-B3z3N0jy.css",
            "assets/index-BM2eU1h0.js.map",
            "assets/chunk-vendor-D3WMmAdf.js",
            "assets/font-a1b2c3d4e5f6.woff2",
        ] {
            assert!(is_content_hashed(hashed), "{hashed}");
        }
        for handwritten in [
            "assets/logo.svg",
            "index.html",
            "assets/logo-icon.svg",
            "assets/some-stylesheet.css",
            "assets/-BM2eU1h0.js",
            "assets/index-BM2eU1h0!.js",
            "assets/index-abcdefgh.js",
            "favicon.ico",
        ] {
            assert!(!is_content_hashed(handwritten), "{handwritten}");
        }
    }

    /// A bundle with no precompressed siblings still serves, which is what a
    /// `--ui-dir` built by an older checkout looks like.
    #[tokio::test]
    async fn a_bundle_with_no_siblings_serves_the_original_to_everyone() {
        let directory = bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());

        let response = serve(
            &source,
            "/app.js",
            &with(header::ACCEPT_ENCODING, "br, gzip"),
        )
        .await
        .expect("app.js");
        assert_eq!(header_of(&response, header::CONTENT_ENCODING), "");
        assert_eq!(body(response).await, "export {};");
    }

    #[test]
    fn a_quality_is_read_and_an_explicit_entry_beats_the_wildcard() {
        assert_eq!(quality("br", "br"), 1.0);
        assert_eq!(quality("gzip, br", "br"), 1.0);
        assert_eq!(quality("BR", "br"), 1.0);
        assert_eq!(quality("br;q=1.0", "br"), 1.0);
        assert_eq!(quality("br; q=0.5", "br"), 0.5);
        assert_eq!(quality("*", "br"), 1.0);
        assert_eq!(quality("br;q=0", "br"), 0.0);
        assert_eq!(quality("br;q=0.000", "br"), 0.0);
        // Not named and no wildcard: not acceptable.
        assert_eq!(quality("gzip", "br"), 0.0);
        assert_eq!(quality("", "br"), 0.0);
        assert_eq!(quality("brotli", "br"), 0.0);
        // An explicit refusal beats a permissive wildcard, whichever order
        // they arrive in, and the wildcard still covers what it does not name.
        assert_eq!(quality("*;q=1, br;q=0", "br"), 0.0);
        assert_eq!(quality("br;q=0, *;q=1", "br"), 0.0);
        assert_eq!(quality("*;q=1, br;q=0", "gzip"), 1.0);
        // A malformed parameter is not a refusal.
        assert_eq!(quality("br;q=banana", "br"), 1.0);
        assert_eq!(quality("br;level=9", "br"), 1.0);
    }

    /// Quality orders the choice, not the order the codings are written in
    /// (issue 043).
    #[tokio::test]
    async fn the_highest_quality_coding_wins() {
        let directory = compressed_bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());

        for (accept, encoding, bytes) in [
            // gzip is preferred outright, even though brotli is stored and
            // would otherwise be tried first.
            ("br;q=0.1, gzip;q=1", "gzip", "gzip bytes"),
            ("gzip;q=0.1, br;q=1", "br", "brotli bytes"),
            // An explicit refusal of brotli leaves gzip, which the wildcard
            // allows: the wildcard never revives what was named and refused.
            ("*;q=1, br;q=0", "gzip", "gzip bytes"),
            // No q anywhere: brotli, because it is the smaller of the two.
            ("gzip, br", "br", "brotli bytes"),
        ] {
            let response = serve(&source, HASHED_PATH, &with(header::ACCEPT_ENCODING, accept))
                .await
                .expect(HASHED_PATH);
            assert_eq!(
                header_of(&response, header::CONTENT_ENCODING),
                encoding,
                "{accept}"
            );
            assert_eq!(body(response).await, bytes, "{accept}");
        }
    }

    /// Every coding refused, `identity` included. RFC 9110 allows a 406; a
    /// wallet that will not load is the worse answer, so the file is served.
    #[tokio::test]
    async fn a_client_refusing_everything_still_gets_the_file() {
        let directory = compressed_bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());

        for accept in ["*;q=0", "identity;q=0, br;q=0, gzip;q=0", "identity;q=0"] {
            let response = serve(&source, HASHED_PATH, &with(header::ACCEPT_ENCODING, accept))
                .await
                .expect(HASHED_PATH);
            assert_eq!(response.status(), StatusCode::OK, "{accept}");
            assert_eq!(
                header_of(&response, header::CONTENT_ENCODING),
                "",
                "{accept}"
            );
            assert_eq!(
                body(response).await,
                "export const original = 1;",
                "{accept}"
            );
        }
    }

    /// A client may spread the tags it holds over several field lines.
    #[tokio::test]
    async fn if_none_match_is_read_across_every_field_line() {
        let directory = compressed_bundle();
        let source = UiSource::Directory(directory.path().to_path_buf());

        let first = serve(&source, "/wallet", &plain()).await.expect("the html");
        let etag = header_of(&first, header::ETAG);

        let mut headers = HeaderMap::new();
        headers.append(
            header::IF_NONE_MATCH,
            HeaderValue::from_static("\"a-tag-from-an-older-build\""),
        );
        headers.append(header::IF_NONE_MATCH, HeaderValue::from_str(&etag).unwrap());
        let response = serve(&source, "/wallet", &headers).await.expect("the html");
        assert_eq!(
            response.status(),
            StatusCode::NOT_MODIFIED,
            "the current tag on the second line still matches"
        );
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
