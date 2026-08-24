# 038: fixtures and contracts for one node API

- Status: open
- Depends on: 037

## Goal

`contracts/` describes one node API: five fixtures removed, four renamed,
seventeen changed, three new, and every `contracts/README.md` statement the
merge made false rewritten in the same change (proposal 006 section 9).

## Scope

- Removed: `http/witness-get-node.json`, `http/witness-get-ledgers.json`,
  `http/witness-get-ledger.json`, `http/witness-get-ledger-events.json`,
  `cli/witness-run.json`.
- Renamed: `http/witness-get-forks.json` to `http/node-get-forks.json`;
  `http/wallet-get-node.json` to `http/node-get-node.json`, shared, with `role`
  out and `witness_for` in; `http/wallet-get-witness-ledgers.json` to
  `http/wallet-get-witness-holdings.json`; `cli/wallet-serve.json` to
  `cli/serve.json`, whose shutdown document gains `witness_for`.
- New: `http/wallet-post-identity-endpoints.json`, `cli/identity-share.json`,
  `cli/identity-endpoints-replace.json`.
- Changed, exactly the rows of the section 9 table: the three identity
  documents with resolved `witnesses`, `endpoints` and `witness_endpoints`;
  paging on `known`; two new `payload_kind` values on the ledger route; identity
  ids and the new refusals on the witnesses route; identity rows with `binding`
  on `wallet-get-witnesses.json`; `?input=`, `input_kind` and `endpoints` on
  resolve; the witness identity, the endpoint and `binding` per row on sync
  push; `from_witness` with `unknown_witness` gone on fetch; the four witness and
  sync CLI documents; the three identity CLI documents; `cli/dev-seed.json`
  creating a witness identity; and `cli/errors.json` gaining `no_local_signer`,
  `invalid_mabel_link`, `unresolvable_witness`, `endpoint_not_identity` and
  `conflicting_source`.
- The `wallet-*` prefix stays on the fixtures that keep it: witnessing adds no
  route, so there is no `witness-*` half left to be symmetrical with.
- `contracts/README.md`: the index rows for the removed, renamed and new
  fixtures, and the ten rewrites section 9 lists, including the payload table
  going to ten rows with `witness_config` marked readable but never written, the
  third amendment to that freeze.

## Acceptance criteria

- [ ] Every fixture in the section 9 table is in its listed class, and the 15
      `http/` and 17 `cli/` documents section 9 calls unchanged are byte
      identical.
- [ ] `contracts/README.md` has an index row for every fixture and no row for a
      file that no longer exists.
- [ ] No statement in `contracts/README.md` still describes a removed route, the
      `role` key, `ledger_not_held`, the `unknown_witness` refusal on fetch or
      the old resolve path.
- [ ] The payload table names ten payloads and records the freeze amendment.
- [ ] tests: the contract tests assert every changed and new fixture body
      against the in-process server, with explicit nulls rather than omitted
      keys.
- [ ] tests: push path unbroken. This ticket changes no admission or resolution
      rule, so `crates/mabel-node/tests/wallet.rs`,
      `crates/mabel-node/tests/witness.rs` and `crates/mabel-cli/tests/sync.rs`
      pass without modification beyond the renamed routes.
- [ ] tests: `cargo fmt`, `clippy` and the workspace suite pass.
