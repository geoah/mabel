# Issues

Implementation tickets for [proposals/001-architecture.md](../proposals/001-architecture.md)
as amended by [proposals/002-unified-ledger.md](../proposals/002-unified-ledger.md)
and [proposals/003-wallet-ux-dns-and-trust-graph.md](../proposals/003-wallet-ux-dns-and-trust-graph.md),
numbered in rough execution order. Template:
[../templates/ticket.md](../templates/ticket.md). Ticket numbers never move;
`Depends on` is the real order, so anything with satisfied dependencies can run
in parallel. Tickets 012, 013 and the whole UI depend on the frozen fixtures in
[../../contracts/](../../contracts/README.md), not on the runtimes, so HTTP, CLI
and UI work proceeds concurrently. Playwright end-to-end tests belong to
milestone 10 and are deliberately not ticketed here.

- [001-workspace-and-proto-schemas.md](001-workspace-and-proto-schemas.md):
  unified protos of proposal 002 section 7, regenerated vectors, workspace fmt,
  clippy and test checks. Depends on none. Done.
- [002-canonical-encoding-and-digests.md](002-canonical-encoding-and-digests.md):
  canonical encoding, digests, base32 ids, signing path, golden vectors.
  Depends on 001. Done.
- [003-wire-format-validator-and-field-table.md](003-wire-format-validator-and-field-table.md):
  descriptor-driven validator, stateless field rows, rejection vectors.
  Depends on 002. Done.
- [004-person-ledger-fold.md](004-person-ledger-fold.md): the fold, state
  boundary, stateful rows, partial validity. Depends on 003. Done.
- [005-membership-fold.md](005-membership-fold.md): invitation lifecycle,
  admission bindings, duplicate keys, promotion, last-controller rule,
  `signing_principal`. Depends on 001. Done.
- [006-file-artifacts-and-fork-records.md](006-file-artifacts-and-fork-records.md):
  the three file artifacts, caps, fork-record validation. Depends on 005.
  Done.
- [007-node-home-and-storage.md](007-node-home-and-storage.md): node home
  layout, typed `node.json`, atomic writes, keys, permissions. Depends on 002.
  Done.
- [008-cli-local-commands.md](008-cli-local-commands.md): CLI skeleton, output
  and exit-code framework, `identity create --kind --founder`, trust, verify,
  `node id`. Depends on 001, 007. Done.
- [009-mabel-net-sync-protocol.md](009-mabel-net-sync-protocol.md): ALPN,
  client, `ProtocolHandler`, caps, loopback tests. Depends on 003. Done.
- [010-witness-runtime.md](010-witness-runtime.md): admission, push semantics,
  fork records, `witness run`, the witness service trait. Depends on 006, 007,
  009. Done.
- [011-wallet-sync-and-multi-source-verify.md](011-wallet-sync-and-multi-source-verify.md):
  push and fetch, append discipline, equivocation, `signing_principal`, the
  sync and verify service traits. Depends on 007, 008, 009. Done.
- [012-http-api-and-loopback-rules.md](012-http-api-and-loopback-rules.md):
  axum routes over the fixtures, service traits, stub answers, loopback
  middleware, UI serving. Depends on 020. Done.
- [013-ui-shell-and-wallet-route.md](013-ui-shell-and-wallet-route.md): Vite
  app shell, fixture-typed API client, identity and trust screens. Depends on
  020. Done.
- [014-witness-ui-route.md](014-witness-ui-route.md): witness debug route with
  ledgers and forks. Depends on 013. Done.
- [015-docker-image-and-compose.md](015-docker-image-and-compose.md):
  multi-stage image and the compose topology with seeded tickets. Depends on
  013. Done.
- [016-cli-integration-and-fresh-verifier.md](016-cli-integration-and-fresh-verifier.md):
  cross-node CLI suite and the fresh-verifier test. Depends on 010, 011, 018.
  Done.
- [017-demo-script.md](017-demo-script.md): CLI demo over the compose
  topology. Depends on 015. Done.
- [018-cli-membership-commands-and-artifacts.md](018-cli-membership-commands-and-artifacts.md):
  `mabel membership invite|accept|admit|remove`, `identity export`, the three
  file artifacts. Depends on 006, 008. Done.
- [019-wallet-principals-and-verify-screens.md](019-wallet-principals-and-verify-screens.md):
  Principals panel, membership, sync and verify screens. Superseded by
  proposal 003's ticket cut and folded into 028.
- [020-api-contract-fixtures.md](020-api-contract-fixtures.md): the frozen
  `contracts/` HTTP and `--json` fixtures. Depends on none. Done.
- [021-membership-http-routes.md](021-membership-http-routes.md): membership
  route fixtures, the node routes, and the wallet wiring. Depends on 012, 018,
  019 (019 superseded by 028). Done.
- [022-mobile-friendly-ui.md](022-mobile-friendly-ui.md): responsive wallet
  and witness UI, identifier truncation, screenshot verification at three
  widths. Depends on 013, 014. Done.
- [023-profile-payload.md](023-profile-payload.md): `ProfileUpdate` at payload
  tag 17, the codepoint policy on `FieldKind::String`, latest-wins fold, the
  `no_op_profile_update` guard, golden and rejection vectors. Depends on none.
  Done.
- [024-dns-hostname-verifier.md](024-dns-hostname-verifier.md): the `Resolver`
  trait over `hickory-resolver`, the `_mabel.<hostname>` TXT rules, the five
  advisory statuses and the verification cache. Depends on 023. Done.
- [025-trust-graph-crawler-and-store.md](025-trust-graph-crawler-and-store.md):
  the `LedgerFetcher` trait and source order, capped breadth-first crawl,
  generations behind `graph/current.json`, reverse edges. Depends on 023, 011.
  Done.
- [026-profile-contact-and-graph-routes.md](026-profile-contact-and-graph-routes.md):
  fixtures first, then the shared identity document, `ResolvedIdentity`, the
  contact store, and the profile, verification, lookup and graph routes with
  their CLI commands. Depends on 023, 024, 025. Done.
- [027-wallet-shell-and-name-resolution.md](027-wallet-shell-and-name-resolution.md):
  identity selector, the `ResolvedIdentity` component and the `Identifier` name
  slot, developer mode, consent panels. Depends on 026. Done.
- [028-identity-view.md](028-identity-view.md): overview table, ledger lines,
  state and actions, absorbing ticket 019's membership, sync and verify
  screens. Depends on 026, 027, 021. Done.
- [029-lookup-and-graph-view.md](029-lookup-and-graph-view.md): foreign-identity
  drill-down, path rendering from the selected root, two-level expansion,
  staleness and truncation surfaces. Depends on 026, 027. Done.
- [030-witness-crawl-provenance.md](030-witness-crawl-provenance.md): witness
  pulls referenced ledgers and records `pull_reason`, off by default. Deferred
  out of the proof of concept. Depends on 025.
- [031-admitted-controller-acts-from-own-home.md](031-admitted-controller-acts-from-own-home.md):
  a fetched ledger whose controller set names a local identity becomes
  actionable from that home. Depends on none. Done.
- [032-topology-tooling-gaps.md](032-topology-tooling-gaps.md): `mabel node
  ticket`, settable node-wide witnesses, a second witness in compose,
  `peers.json` hints on push. Depends on none. Done.
- [033-witness-set-and-endpoint-advertisement-payloads.md](033-witness-set-and-endpoint-advertisement-payloads.md):
  payload tags 19 `WitnessSet` and 18 `EndpointAdvertisement`, the three fold
  accessors, the builders with `build_witness_config` test-gated, the routes and
  commands that write them, `witness_for` and the tag-19 admission clause,
  golden and rejection vectors. Depends on none.
- [034-admission-witness-for-and-bindings.md](034-admission-witness-for-and-bindings.md):
  the four-clause admission rule over the pre-push and pushed state, the gated
  legacy tag-11 clause, the advertisement invariant on `witness_for`,
  `bindings/<identity_id>.json` and the verified predicate, the witness-ledger
  fetch from another endpoint. Depends on 033.
- [035-resolution-sources-and-dial-budget.md](035-resolution-sources-and-dial-budget.md):
  the eight fetch sources, witness resolution as a non-recursive base operation,
  the visited-identity set, the 16-endpoint dial budget with its per-class caps
  and shared deadline, `peers.json` objects with cap, age-out and eviction, the
  `node.json` bootstrap endpoints. Depends on 034.
- [036-mabel-links-and-dns-endpoint-hints.md](036-mabel-links-and-dns-endpoint-hints.md):
  the `mabel://` grammar in core with vectors, the decode-once rule, the
  `mabel-endpoints=` TXT key with discard-whole overflow and the applicability
  matrix, `GET /api/resolve?input=`, `mabel identity share` with QR and file
  output. Depends on 033.
- [037-one-router-and-one-store.md](037-one-router-and-one-store.md): `api::wallet`
  and `api::witness` merged, the two runtimes merged, `WalletReadStore` deleted
  for `node::LedgerStorage`, `/api/ledgers*` folded into the identity routes,
  paging on `known`, the `holdings` segment, the `List` narrowing, `mabel serve`,
  `role` recognised and ignored. Depends on 035, 036.
- [038-fixtures-and-contracts.md](038-fixtures-and-contracts.md): the five
  removed, four renamed, three new and seventeen changed fixtures of the section
  9 table, the `contracts/README.md` rewrites, the payload table at ten rows.
  Depends on 037.
- [039-witnesses-as-identity-cards-in-the-ui.md](039-witnesses-as-identity-cards-in-the-ui.md):
  `/witness` and `WitnessCard.tsx` removed, `/witnesses` drawing identity cards,
  the machines row and its two sentences, the share and machines actions with
  their consent text, the mock store and UI tests. Depends on 038.
- [040-topology-and-stories.md](040-topology-and-stories.md): entrypoint witness
  identity and advertisement, `MABEL_WITNESSES` as ids with endpoints, the third
  published file, the compose overlays and zone files, four story rewrites and
  two new stories for a link with no witness and an endpoint rotation. Depends
  on 039.
- [041-mabel-id-prefix.md](041-mabel-id-prefix.md): `mabel://` on every shown
  identity id with bare ids on machine surfaces, id fields that take the
  prefixed form, the `mabel-endpoints=` line on the handle screen, endpoints as
  the one noun for advertised machines, a vendored `Tabs` used by the two
  filtered lists, identity card polish, and a named witness from `dev seed` and
  the compose entrypoint. Depends on 040.
