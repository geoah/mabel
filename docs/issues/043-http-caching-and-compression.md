# 043: no `Cache-Control`, no `ETag` and no compression on any response

- Status: open
- Depends on: 041

## Goal

A repeat page load transfers headers, not the bundle. An upgraded image does
not break the browsers that were holding the old one. No intermediary keeps a
copy of a wallet document.

## Problem

Every response this node writes carries `Content-Type` and nothing else.
`api::ui::asset` set one header and the router had one layer, the loopback
rules.

- `/assets/index-*.js` is 386874 bytes and is re-downloaded on every load. The
  filename holds a content hash, so those bytes could be cached for a year.
- `index.html` has no `ETag`, so a browser caches it on a heuristic. After an
  image upgrade it asks the new binary for the hashed asset the old html named,
  which is gone, and gets a bare 404: the UI stays broken until someone hard
  refreshes.
- No response is compressed. The bundle is 386874 bytes on the wire where
  brotli is 99443.
- No API response says how it may be cached, so an intermediary may serve a
  heuristically fresh copy of an identity document. That is the mechanism
  behind the stale document reported in issue 042.

## Scope

- `ui/precompress.ts`, a Vite plugin writing a `.br` and a `.gz` beside every
  compressible file over 256 bytes. The siblings are embedded with the rest of
  `ui/dist`, so a request picks a stored representation and nothing compresses
  per request. Node's own `zlib` does it: no new npm dependency, no `build.rs`,
  and rust-embed needs no change.
- `api::ui::serve` takes the request headers and answers with `Cache-Control`,
  `ETag` and `Vary: Accept-Encoding`; `public, max-age=31536000, immutable`
  under `assets/` and `no-cache` everywhere else, `304` to a matching
  `If-None-Match`, and `Content-Encoding: br` or `gzip` when the client takes
  one and the sibling exists. A client offering no encoding gets the file.
- The `ETag` is the sha256 of the original bytes, from rust-embed's
  compile-time metadata for the embedded bundle and computed on read for
  `--ui-dir`, suffixed per encoding so a cache cannot cross representations.
- `tower_http::CompressionLayer` for the JSON routes, and one middleware
  putting `Cache-Control: no-store` on every `/api` answer. Both sit inside the
  loopback layer, so a 403 or 415 envelope is byte for byte what it was.
- Nothing reads `Host`, and `Vary` names `Accept-Encoding` alone: the deployed
  reverse proxy and `--allow-host` are unaffected.

## Acceptance criteria

- [ ] A hashed asset answers `public, max-age=31536000, immutable`; `/` and
      every SPA route answer `no-cache` with an `ETag`, and a matching
      `If-None-Match` answers 304 with no body.
- [ ] `Accept-Encoding: br` and `gzip` get the stored siblings with the right
      `Content-Encoding`; no `Accept-Encoding` gets the original bytes. Every
      asset answer carries `Vary: Accept-Encoding`.
- [ ] Every `/api` answer carries `Cache-Control: no-store`, and no answer
      varies on `Host`.
- [ ] tests: `cargo fmt`, `clippy`, the workspace suite, the UI suite, the UI
      build and lint, and the full Playwright suite are green.
