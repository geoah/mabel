# 013: UI app shell, API client and the identity and trust screens

- Status: done
- Depends on: 020

## Goal

One Vite app under `ui/` with the two routes of proposal 001 section 10, built
as a single bundle the node embeds, carrying the shared shell, the API client
and the wallet's identity and trust screens. It is written against
`contracts/http/`, not against the node, so it proceeds in parallel with
tickets 010 to 012.

## Scope

- `ui/`: React 19.2.8, Vite 8.2.2, Tailwind 4.3.3, TypeScript 7.0.2 and
  shadcn/ui vendored in, one source tree, two routes (wallet and witness),
  shared components, one bundle (sections 10 and 12).
- Routing, layout, error and loading states, and a typed API client whose
  request and response types come from the `contracts/http/` fixtures; its
  `?since=` parameters are inclusive. Every fixture key exists on the type,
  including the ones that are `null`.
- Wallet screens in this ticket: node info, identity list and creation with
  `declared_kind`, identity detail with its ledger, witness configuration,
  trust add, revoke and list.
- Verification results rendered from the API's report struct, including the flag
  L `subject_control` sentence and the flag R `source`, `head_seq`,
  `head_event` and `fetched_at_ms` fields the node returns (section 6).
- The error envelope rendered from `code` and `details.reason`, never from
  `message` (`contracts/README.md`).
- Stable `data-testid` attributes on the elements a later suite drives.
- Build wiring: the production build lands where `rust-embed` picks it up, and
  `--ui-dir` serves the dev build (section 10).
- Witness route placeholder only (ticket 014). The Principals panel, membership,
  sync and verify screens are ticket 019.

Out of scope: Playwright specs, which belong to milestone 10 and are
deliberately not ticketed here. Nothing here waits on ticket 012: tests run
against the fixtures, and the ticket 012 stub server serves them unchanged.

## Acceptance criteria

- [x] The app holds no keys and performs no crypto; every operation is an API
      call (section 10).
- [ ] The API client types are derived from `contracts/http/` and a fixture
      document parses into them with no missing or extra key.
- [ ] One build produces one bundle covering both routes, and the pinned
      dependency versions match section 12.
- [x] Interactive elements carry `data-testid` attributes.
- [x] tests: vitest plus testing-library component tests with the fixtures as
      the mocked API cover the identity and trust forms, their validation and
      the rendering of an error envelope.
- [ ] tests: `npm run build` succeeds, typecheck and lint pass, and the built
      bundle is served by `wallet serve` both from the embed and via
      `--ui-dir`.
