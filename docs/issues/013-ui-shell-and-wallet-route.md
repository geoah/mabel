# 013: UI app shell, API client and the identity and trust screens

- Status: open
- Depends on: 012

## Goal

One Vite app under `ui/` with the two routes of proposal 001 section 10, built
as a single bundle the node embeds, carrying the shared shell, the API client
and the wallet's identity and trust screens.

## Scope

- `ui/`: React 19.2.8, Vite 8.2.2, Tailwind 4.3.3, TypeScript 7.0.2 and
  shadcn/ui vendored in, one source tree, two routes (wallet and witness),
  shared components, one bundle (sections 10 and 12).
- Routing, layout, error and loading states, and a typed API client for the
  ticket 012 routes; its `?since=` parameters are inclusive.
- Wallet screens in this ticket: node info, identity list and creation,
  identity detail with its ledger, witness configuration, trust add, revoke and
  list.
- Verification results rendered from the API's "as of seq N from source S"
  struct, including the flag L and flag R wording the node returns (section 6).
- Stable `data-testid` attributes on the elements a later suite drives
  (section 10).
- Build wiring: the production build lands where `rust-embed` picks it up, and
  `--ui-dir` serves the dev build (section 10).
- Witness route placeholder only (ticket 014); org, sync and verify screens are
  ticket 019.

Out of scope: Playwright specs, which belong to milestone 10 and are
deliberately not ticketed here.

## Acceptance criteria

- [ ] The app holds no keys and performs no crypto; every operation is an API
      call (section 10).
- [ ] One build produces one bundle covering both routes (section 10).
- [ ] The pinned dependency versions match section 12.
- [ ] Interactive elements carry `data-testid` attributes.
- [ ] tests: vitest plus testing-library component tests with a mocked API
      cover the identity and trust forms, their validation and the rendering of
      an API error.
- [ ] tests: `npm run build` succeeds, typecheck and lint pass, and the built
      bundle is served by `wallet serve` both from the embed and via
      `--ui-dir`.
