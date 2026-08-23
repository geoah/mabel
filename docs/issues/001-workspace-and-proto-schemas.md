# 001: workspace, proto schemas and mabel-proto

- Status: open
- Depends on: none

## Goal

A building Cargo workspace with the crate layout of proposal 001 section 7, the
three normative `.proto` files, a `mabel-proto` crate that generates types from
them, and a confirmed answer to the milestone-1 `iroh-base` key question
(section 4).

## Scope

- Workspace root: MSRV 1.91, edition 2024, member crates `mabel-proto`,
  `mabel-core`, `mabel-net`, `mabel-node`, `mabel-cli`, plus empty `ui/`,
  `docker/`, `test-vectors/`, `tests/e2e/` directories (section 7).
- Workspace dependency table pinning the versions listed in section 12.
- `proto/mabel/v0/ledger.proto`, `sync.proto`, `files.proto` transcribing the
  messages in sections 3.2, 3.4, 3.5, 5 and 3.8. The proposal sketches the sync
  messages without field numbers; assign them and keep the `oneof` tags it does
  fix (payload 10 to 17, `Request` 1 to 5, `Response` 1 to 7).
- `crates/mabel-proto`: `build.rs` using `prost-build` with
  `protoc-bin-vendored`, generated types re-exported, no other code.
- One task or script running `cargo fmt --check`, `cargo clippy --all-targets
  -- -D warnings` and `cargo test --workspace`.
- Root `README.md` carrying the "verified means" sentence from section 1 and the
  flag L sentence from section 6.
- Milestone-1 probe: compile a call to `iroh_base`'s `SecretKey::from_bytes`
  under `default-features = false, features = ["key"]`. If that pulls runtime
  dependencies into `mabel-core`, take the section 4 fallback (`ed25519-dalek`
  inside iroh's `>=3.0.0-rc.0,<4.0.0` range) instead.

## Acceptance criteria

- [ ] `cargo build --workspace` succeeds on Rust 1.91 with edition 2024.
- [ ] The three `.proto` files contain every message named in sections 3.2,
      3.4, 3.5, 3.8 and 5, with the field numbers and `oneof` tags those
      sections fix, and are append-only from this point (section 3.1).
- [ ] `mabel-proto` contains only `build.rs` and re-exports.
- [ ] `mabel-core` compiles against the key crate chosen by the probe; the
      chosen feature set or fallback is recorded in a comment in the workspace
      `Cargo.toml`.
- [ ] `cargo tree -p mabel-core` lists neither `tokio` nor `iroh` proper
      (section 7).
- [ ] Root `README.md` contains both sentences verbatim from sections 1 and 6.
- [ ] tests: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
      and `cargo test --workspace` all pass.
