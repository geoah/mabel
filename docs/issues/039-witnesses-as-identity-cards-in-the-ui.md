# 039: witnesses as identity cards in the UI

- Status: open
- Depends on: 038

## Goal

The UI draws a witness with the same identity card as everyone else: the
`/witness` route tree is gone, `/witnesses` lists identities, the identity page
gains a machines row with share and machine actions, and every node runs the
same three nav entries (proposal 006 section 8).

## Scope

- Fix the invalid nesting the e2e run found: the info icon inside
  `lookup-trust-toggle` and `lookup-reverse-toggle` is a button inside a
  button; the tooltip trigger becomes a non-button element with the same
  keyboard behavior.

- Deleted: `ui/src/components/WitnessCard.tsx`, `ui/src/routes/witness/` and
  `ui/src/routes/witnesses/WitnessLedgersPage.tsx`, with the notes of
  `routes/witness/notes.ts` moving to the "Known identities" section.
  `nav-witness` and the role branch in `App.tsx` go with them.
- Nav is `nav-wallet`, `nav-witnesses` and `nav-node` on every node. A witness's
  home page is the wallet home page: its own identity under "Your identities",
  its holdings under "Known identities". `/witnesses` draws identity cards, and
  the facts the witness card carried move onto the witness identity's own page
  as rows and a "What this witness holds" section.
- `/node` loses `node-role` and gains `node-witness-for`, reading `none` when
  this home witnesses for nobody, and states in one sentence that this home
  holds no keys and what it does hold when `GET /api/identities` is empty.
- The identity page gains a `machines` row and the actions `action-endpoints`
  and `action-share`. The machines row is one row per machine with labelled
  values, the id in the same lowercase base32 the rest of the UI uses with no
  separators inserted, and a full sentence on its own line saying either that
  the machine is listed on this identity's own record or that no record we have
  confirms it. `binding`, `verified`, `hinted` and `endpoints` never appear in UI
  copy.
- `action-share` shows the link with a copy control, the same string as a QR
  square, and a `.mabel` file download, and says what handing it over discloses:
  the identity id, the machines that answer for it, and this home's address to
  whoever uses it.
- `action-endpoints` asks for consent once per home through the panel
  `handle-consent` already establishes, stating the three facts of section 8:
  the machine id stays readable forever, anyone who reads it can dial that
  machine, and once this home answers at a published address anyone who dials it
  can list the identities it signs for and the ledgers it keeps.
- `wallet-search` is relabelled `Mabel ID, handle or link`. Pasting a link
  navigates to the identity page and passes its machines to the fetch as caller
  hints; before the fetch runs the page states what using the link does. Pasting
  a bare id navigates with no hints. The browser never parses a link itself: it
  calls `GET /api/resolve?input=`.
- `WitnessConfigPanel.tsx` names identities and keeps its read-modify-write.
  `api/types.ts` retypes `WitnessSummary` and `SetWitnessesRequest`. The mock
  store and the UI tests follow the fixtures of ticket 038.

## Acceptance criteria

- [ ] No `witness-detail-*` testid, no `/witness` route and no role branch
      remains in `ui/`.
- [ ] An identity card draws its actions only when the identity appears in `GET
      /api/identities`, so a home with no keys shows rows and no buttons.
- [ ] No UI copy contains `endpoints`, `binding`, `verified`, `hinted`, a middle
      dot or a dash joining an id to a status.
- [ ] tests: UI tests for the machines row in both sentences, the share panel,
      the consent panel, `node-witness-for` reading `none`, and a link pasted
      into the search box.
- [ ] tests: screenshot verification at the three widths ticket 022 uses, for
      `/witnesses`, the witness identity page and the share panel.
- [ ] tests: push path unbroken. This ticket changes no admission or resolution
      rule; the cargo suites and `crates/mabel-cli/tests/sync.rs` pass
      unmodified, and the UI suite and `cargo fmt` and `clippy` are green.
