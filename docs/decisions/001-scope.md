# 001: POC scope and deliverables

- Date: 2026-08-23
- Status: accepted
- Source: product owner

- Mabel is a proof of concept. It must run end-to-end, stay small, and stay
  clean. Do not build for production key custody.
- Rust library ("core") that powers every node type: user wallet,
  organization handling, witness. One set of primitives shared by all.
- Deliverables: core library, CLI, user wallet web UI (create and manage
  person identities and organizations), witness node with a debug UI listing
  the ledgers it holds, container images, end-to-end tests that drive the
  CLI and web UIs through realistic user stories.
- Native-only for now. No wasm or mobile builds in this POC, but do not bake
  in choices that make them unreasonable later.
