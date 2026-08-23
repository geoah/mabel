# Issues

Implementation tickets for [proposals/001-architecture.md](../proposals/001-architecture.md)
as amended by [proposals/002-unified-ledger.md](../proposals/002-unified-ledger.md),
numbered in rough execution order. Template:
[../templates/ticket.md](../templates/ticket.md). Ticket numbers never move;
`Depends on` is the real order, so anything with satisfied dependencies can run
in parallel. Tickets 012, 013 and the whole UI depend on the frozen fixtures in
[../../contracts/](../../contracts/README.md), not on the runtimes, so HTTP, CLI
and UI work proceeds concurrently. Playwright end-to-end tests belong to
milestone 10 and are deliberately not ticketed here.

- [001-workspace-and-proto-schemas.md](001-workspace-and-proto-schemas.md):
  unified protos of proposal 002 section 7, regenerated vectors, workspace fmt,
  clippy and test checks. Depends on none.
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
  `signing_principal`. Depends on 001.
- [006-file-artifacts-and-fork-records.md](006-file-artifacts-and-fork-records.md):
  the three file artifacts, caps, fork-record validation. Depends on 005.
- [007-node-home-and-storage.md](007-node-home-and-storage.md): node home
  layout, typed `node.json`, atomic writes, keys, permissions. Depends on 002.
  Done.
- [008-cli-local-commands.md](008-cli-local-commands.md): CLI skeleton, output
  and exit-code framework, `identity create --kind --founder`, trust, verify,
  `node id`. Depends on 001, 007.
- [009-mabel-net-sync-protocol.md](009-mabel-net-sync-protocol.md): ALPN,
  client, `ProtocolHandler`, caps, loopback tests. Depends on 003. Done.
- [010-witness-runtime.md](010-witness-runtime.md): admission, push semantics,
  fork records, `witness run`, the witness service trait. Depends on 006, 007,
  009.
- [011-wallet-sync-and-multi-source-verify.md](011-wallet-sync-and-multi-source-verify.md):
  push and fetch, append discipline, equivocation, `signing_principal`, the
  sync and verify service traits. Depends on 007, 008, 009. Done.
- [012-http-api-and-loopback-rules.md](012-http-api-and-loopback-rules.md):
  axum routes over the fixtures, service traits, stub answers, loopback
  middleware, UI serving. Depends on 020.
- [013-ui-shell-and-wallet-route.md](013-ui-shell-and-wallet-route.md): Vite
  app shell, fixture-typed API client, identity and trust screens. Depends on
  020.
- [014-witness-ui-route.md](014-witness-ui-route.md): witness debug route with
  ledgers and forks. Depends on 013.
- [015-docker-image-and-compose.md](015-docker-image-and-compose.md):
  multi-stage image and the compose topology with seeded tickets. Depends on
  013.
- [016-cli-integration-and-fresh-verifier.md](016-cli-integration-and-fresh-verifier.md):
  cross-node CLI suite and the fresh-verifier test. Depends on 010, 011, 018.
  Done.
- [017-demo-script.md](017-demo-script.md): CLI demo over the compose
  topology. Depends on 015.
- [018-cli-membership-commands-and-artifacts.md](018-cli-membership-commands-and-artifacts.md):
  `mabel membership invite|accept|admit|remove`, `identity export`, the three
  file artifacts. Depends on 006, 008.
- [019-wallet-principals-and-verify-screens.md](019-wallet-principals-and-verify-screens.md):
  Principals panel, membership, sync and verify screens. Depends on 013, plus
  021's contract freeze for the membership screens.
- [020-api-contract-fixtures.md](020-api-contract-fixtures.md): the frozen
  `contracts/` HTTP and `--json` fixtures. Depends on none. Done.
- [021-membership-http-routes.md](021-membership-http-routes.md): membership
  route fixtures, the node routes, and the wallet wiring. Depends on 012, 018,
  019.
- [022-mobile-friendly-ui.md](022-mobile-friendly-ui.md): responsive wallet
  and witness UI, identifier truncation, screenshot verification at three
  widths. Depends on 013, 014.
