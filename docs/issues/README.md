# Issues

Implementation tickets for [proposals/001-architecture.md](../proposals/001-architecture.md),
numbered in rough execution order. Template:
[../templates/ticket.md](../templates/ticket.md). Ticket numbers never move;
`Depends on` is the real order, so anything with satisfied dependencies can run
in parallel. Playwright end-to-end tests belong to milestone 10 and are
deliberately not ticketed here.

- [001-workspace-and-proto-schemas.md](001-workspace-and-proto-schemas.md):
  workspace, `.proto` files, `mabel-proto`, toolchain, `iroh-base` key check.
  Depends on none.
- [002-canonical-encoding-and-digests.md](002-canonical-encoding-and-digests.md):
  canonical encoding, digests, base32 ids, signing path, golden vectors.
  Depends on 001.
- [003-wire-format-validator-and-field-table.md](003-wire-format-validator-and-field-table.md):
  descriptor-driven validator, stateless field rows, rejection vectors.
  Depends on 002.
- [004-person-ledger-fold.md](004-person-ledger-fold.md): the fold, state
  boundary, stateful rows, person payloads, partial validity. Depends on 003.
- [005-org-ledger-fold.md](005-org-ledger-fold.md): embedded inceptions,
  invites, acceptance, removal, promotion. Depends on 004.
- [006-file-artifacts-and-fork-records.md](006-file-artifacts-and-fork-records.md):
  the three file artifacts, caps, fork-record validation. Depends on 005.
- [007-node-home-and-storage.md](007-node-home-and-storage.md): node home
  layout, typed `node.json`, atomic writes, keys, permissions. Depends on 002.
- [008-cli-local-commands.md](008-cli-local-commands.md): CLI skeleton, output
  and exit-code framework, identity, trust, verify, `node id`. Depends on 005,
  007.
- [009-mabel-net-sync-protocol.md](009-mabel-net-sync-protocol.md): ALPN,
  client, `ProtocolHandler`, caps, loopback tests. Depends on 003.
- [010-witness-runtime.md](010-witness-runtime.md): admission, push semantics,
  fork records, `witness run`. Depends on 006, 007, 009.
- [011-wallet-sync-and-multi-source-verify.md](011-wallet-sync-and-multi-source-verify.md):
  push and fetch, append discipline, equivocation. Depends on 007, 008, 009,
  018.
- [012-http-api-and-loopback-rules.md](012-http-api-and-loopback-rules.md):
  axum APIs, loopback middleware, UI serving. Depends on 010, 011.
- [013-ui-shell-and-wallet-route.md](013-ui-shell-and-wallet-route.md): Vite
  app shell, API client, identity and trust screens. Depends on 012.
- [014-witness-ui-route.md](014-witness-ui-route.md): witness debug route with
  ledgers and forks. Depends on 013.
- [015-docker-image-and-compose.md](015-docker-image-and-compose.md):
  multi-stage image and the compose topology with seeded tickets. Depends on
  013.
- [016-cli-integration-and-fresh-verifier.md](016-cli-integration-and-fresh-verifier.md):
  cross-node CLI suite and the fresh-verifier test. Depends on 010, 011.
- [017-demo-script.md](017-demo-script.md): CLI demo over the compose
  topology. Depends on 015.
- [018-cli-org-commands-and-artifacts.md](018-cli-org-commands-and-artifacts.md):
  org CLI commands, `identity export`, the three file artifacts. Depends on
  006, 008.
- [019-wallet-org-and-verify-screens.md](019-wallet-org-and-verify-screens.md):
  wallet org, sync and verify screens. Depends on 013.
