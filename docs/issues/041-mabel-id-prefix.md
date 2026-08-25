# 041: shown identity ids carry `mabel://`, and endpoints get their name

- Status: open
- Depends on: 040

## Goal

An identity id put in front of a person reads `mabel://<id>` on every surface,
while machine surfaces keep the bare id (decision 019). The machines an identity
advertises are called endpoints in prose as well as on the wire (decision 020).
The handle screen shows the second DNS line proposal 006 section 6 defines, the
two filtered lists become tabs, and a seeded witness gets a name.

## Scope

- `ui/src/lib/link.ts` gains `MABEL_PREFIX`, `mabelId` and `identityIdInput`.
  `Identifier` gains a `mabel` prop: the visible text and the copy control take
  the prefix, `data-value` keeps the bare id, and the id alone is what gets
  split when a value is truncated.
- Every UI site that shows an identity id sets it: `IdentityInline`, which is
  what every card and every inline identity draws through, plus the create
  result, the push report, the QR label and the two membership selects, whose
  option values stay bare.
- `EventLines` renders entry contents with the identity ids in them prefixed,
  keyed by `payload_kind` and not by field name: `target` is an identity under
  `membership_removal` and an entry id under `trust_revocation`, and
  `witnesses` names identities under `witness_set` and endpoints under the
  retired `witness_config`.
- Every UI box that takes an identity id takes the prefixed form: witness add,
  trust add, trust revoke and the founder box. A link naming endpoints is
  refused there rather than stripped of them.
- `crates/mabel-cli/src/ids.rs` gains `shown`, built from `mabel_core::LINK_PREFIX`,
  and every CLI text line that prints an identity id uses it. `--json`
  documents are untouched.
- `Context::resolve` and `Context::resolve_local_hinted` refuse a link carrying
  `?endpoints=` with reason `invalid_mabel_link` instead of dropping the
  endpoints. `resolve_hinted` still accepts one, for `witness add --witness` and
  `sync fetch`, which dial.
- `HandlePanel` shows the `mabel-endpoints=` line beside the `mabel=` line when
  the identity advertises endpoints, with a sentence saying what each line does.
  The ids inside a record value stay bare.
- The endpoints noun replaces "machines" and "nodes" in UI copy, entry glosses,
  CLI text, the stories, `contracts/README.md`, `docker/README.md` and the demo
  docs.
- A vendored `Tabs` component in the hand-built no-Radix style of the other
  `ui/src/components/ui` files, used in exactly two places: Known identities
  (All, Trusted) and witness holdings (All, Trusted, Yours).
- Identity card polish: the expand control matches the pill height, the kind
  pill leads the pill row, the copy control shrinks, and a card with no display
  name titles itself with the first eight characters of the id and an ellipsis.
- `mabel dev seed` and `docker/entrypoint.sh` give the witness identity they
  mint a display name, so a witness shows a name rather than an id.
- `docs/decisions/019-mabel-id-prefix.md` and
  `docs/decisions/020-endpoints-not-machines.md`.

## Acceptance criteria

- [ ] No screen and no CLI text line shows a bare identity id, and no
      `data-value`, `--json` document, HTTP body, `node.json` or `peers.json`
      shows a prefixed one.
- [ ] An endpoint id is bare everywhere, under its own label.
- [ ] A bare id and the same id with its prefix are one input on every field
      and flag that takes an identity, and a link naming endpoints is refused
      whole where nothing dials, with reason `invalid_mabel_link`.
- [ ] The handle screen shows both DNS lines for an identity that advertises
      endpoints and the first alone for one that does not.
- [ ] The word "machines" appears in no visible copy, and "node" means the
      running program alone.
- [ ] The two tab strips work by keyboard: arrow keys move and activate, Home
      and End jump, and only the selected tab is in the tab order.
- [ ] tests: `cargo fmt`, `clippy`, the workspace suite, the UI suite, the UI
      build and the full Playwright suite are green, and the fixture index test
      still names every fixture.
