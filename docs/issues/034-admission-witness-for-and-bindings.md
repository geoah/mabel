# 034: admission, `witness_for` and endpoint bindings

- Status: open
- Depends on: 033

## Goal

A witness admits an extension only while a live witness set names an identity it
witnesses for, only while that identity's chain advertises this machine, and a
pusher can tell whether the endpoint it dialled is one the witness identity
actually named (proposal 006 sections 4, 4.1 and 4.2).

## Scope

- The compose bridge, so the topology never loses its push path mid-sequence:
  `docker/entrypoint.sh` creates a witness identity in the witness home, sets
  `witness_for` to it and `accept_legacy_witness_config: true`, so the
  existing tag-11 stories keep pushing until ticket 040 modernizes them. One
  smoke run proves it.

- `crates/mabel-node/src/witness/storage.rs`: the four clauses of section 4 in
  order, over `pre`, the folded state of the copy this home stores, and `post`,
  the folded state of the pushed chain. This replaces the `held` branch's
  implicit admission, so a witness once named no longer accepts pushes forever.
  Reads stay open to all.
- `crates/mabel-node/src/config.rs`: `accept_legacy_witness_config`, default
  false, documented as a migration switch, and clause 4's triple gate: a
  non-empty `witness_for`, the switch on, and this node's own endpoint id in
  `pre.witness_endpoints()` or `post.witness_endpoints()`.
- The 4.1 invariant: a `witness_for` entry admits new ledgers only while the
  latest non-equivocating local copy of that identity folds to an `endpoints()`
  holding this home's `node.key` public half. Checked at startup, on storing a
  longer copy of that identity, and when this home's endpoint id changes. A
  failing entry stops clause 3 for itself alone, is reported on `GET /api/node`
  beside the id, and is named once in the startup log with the reason.
- New `crates/mabel-node/src/bindings.rs` and the `bindings/` path in
  `home.rs`: `bindings/<identity_id>.json` in the shape section 4.2 gives, with
  the head-seq rules, the equivocation clearing, and the file as a deletable
  derived cache. The crawler still writes no stranger's ledger under `ledgers/`.
- The 4.2 predicate, including condition 4: a chain served only by the endpoint
  it vouches for leaves that endpoint `hinted`.
- `crates/mabel-node/src/wallet/sync.rs`: after a push that stored events, one
  `Get` for the witness identity's own ledger, from an endpoint other than the
  one just pushed to, its result checked against the requested ledger id. Report
  `binding` per endpoint; `mabel sync push` prints, for a `hinted` endpoint,
  that nobody's ledger confirms it. A hinted binding is never a refusal.

## Acceptance criteria

- [ ] Clause 2 admits the very event that drops this witness, and the next
      extension after it is refused; the stored prefix is kept and still served
      on read.
- [ ] Clause 3 admits a first push where `pre` is empty; an empty `WitnessSet`
      stops every later extension.
- [ ] A home with an empty `witness_for` refuses a push for a ledger it does not
      store, with the reason naming the rule and not the program.
- [ ] Startup never fails on a `witness_for` entry whose advertisement has not
      landed; `GET /api/node` names the entry and one of the three reasons.
- [ ] tests: the four clauses in `crates/mabel-node/tests/witness.rs`, with
      `accept_legacy_witness_config` covered both on and off and with each leg of
      its triple gate refused.
- [ ] tests: a failing 4.1 entry still admits an extension for a stored ledger
      and refuses a new one.
- [ ] tests: a binding is not created from evidence served by its own endpoint;
      a lower head seq neither creates nor refreshes; an equal head seq with
      divergent events clears every binding for that identity; a higher head seq
      replaces the entry list and drops an absent endpoint back to `hinted`.
- [ ] tests: push path unbroken. `crates/mabel-cli/tests/sync.rs` pushes and
      fetches across two homes, and a hinted push succeeds with the warning on
      stderr; `cargo fmt`, `clippy` and the workspace suite pass.
