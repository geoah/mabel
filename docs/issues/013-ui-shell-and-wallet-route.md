# 013: UI app shell and wallet route

- Status: open
- Depends on: 012

## Goal

One Vite app under `ui/` with the two routes of proposal 001 section 10, built
as a single bundle the node embeds, with the wallet route driving every wallet
API operation.

## Scope

- `ui/`: React 19, Vite, TypeScript, Tailwind and shadcn/ui vendored in, one
  source tree, two routes (wallet and witness), shared components, one bundle
  (sections 10 and 12).
- Wallet route: node info, identity list and creation, identity detail with its
  ledger, witness configuration, trust add and revoke, org creation, invite,
  acceptance, admit and removal, sync push, and verification, each calling the
  ticket 012 API.
- Verification results rendered from the API's "as of seq N from source S"
  struct, including the flag L and flag R wording the node returns (section 6).
- Stable `data-testid` attributes on the elements a later e2e suite drives
  (section 10).
- Build wiring: the production build lands where `rust-embed` picks it up, and
  `--ui-dir` serves the dev build (section 10).
- Witness route placeholder only; its content is ticket 014.

Out of scope: Playwright specs, which are phase 6.

## Acceptance criteria

- [ ] The app holds no keys and performs no crypto; every operation is an API
      call (section 10).
- [ ] One build produces one bundle covering both routes (section 10).
- [ ] Every wallet API route from section 10 is reachable from the UI.
- [ ] Interactive elements carry `data-testid` attributes.
- [ ] tests: `npm run build` succeeds, TypeScript typecheck and lint pass, and
      the built bundle is served by `wallet serve` with and without `--ui-dir`.
