# 004: the three-primitive UI

- Date: 2026-08-25
- Status: accepted (owner direction, 2026-08-25)
- Decisions affected: amends the wallet shape of 014; supersedes the
  screen layout of proposal 003 sections 4 and 6 while keeping their
  rendering rules (names never render as ids, staleness is shown,
  reverse edges stay best effort)

## Context

The shipped wallet has four tabs, a radio-button identity selector whose
selection changes nothing the user can see, a lookup screen that
duplicates the identity view, a verify tab nobody can explain, and a
witness debug route that reads as an operator dump. The owner's ruling:
the whole UI is three primitives and nothing else.

## Proposal

The three primitives:

1. **A list of identities.** One card per identity: resolved name with
   the verification mark, the id, declared kind, head seq. The whole
   card is a link to the identity page. No radio buttons, no selection
   state. The same card list renders everywhere a list of identities
   appears.
2. **A list of witnesses.** One card per known witness endpoint.
3. **The identity page.** One page for every identity, local or
   foreign, at `/identities/<id>`. What varies is a single fact: when
   the wallet can sign for the identity it carries a "your identity"
   badge and the Actions section; otherwise it shows the contact note
   editor and how you know them (path, degrees, per-hop staleness).
   Everything else (overview, profile, trust list, principals, ledger)
   renders identically from whatever the wallet holds.

The screens:

- `/wallet`: a search box, the identity card list, and a create form.
  The search box takes an identity id or a hostname. An id navigates to
  the identity page. A hostname resolves through the node
  (`GET /api/resolve/<hostname>`) and navigates to the resolved id, or
  says what the TXT lookup answered.
- `/identities/<id>`: the identity page. When the ledger is not stored
  locally the page offers one action: fetch it from a known witness
  (`POST /api/identities/<id>/fetch`), after which it renders like any
  other. Graph knowledge (path, trust list, reverse edges) renders
  whenever the crawl holds the identity, stored or not.
- `/witnesses`: the witness card list. A card names the endpoint id and
  where the wallet knows it from.
- `/witnesses/<endpoint_id>`: what that witness holds, fetched live
  over the sync protocol's existing `List` request and rendered as the
  identity card list. Clicking a card goes to the identity page.
- Nav is two entries: Wallet and Witnesses. The verify tab, the lookup
  tab, the identity selector and the standalone lookup screens are
  removed. Verification of trust and ledgers stays a CLI concern.
- The witness node's own debug route (`/witness`, served by the witness
  binary) becomes the same two screens read-only: its held-ledger card
  list and the identity page. The operator tables, paging controls and
  fork cards leave the UI; forks stay visible as a count on the card
  and a section on the identity page when records exist.

New routes, wallet side, all loopback like the rest:

- `GET /api/witnesses`:
  `{ok, witnesses: [{endpoint_id, named_by: [identity ids whose folded
  witness config names it], is_node_default: bool}]}`. Sources: folded
  witness configs of stored ledgers plus `node.json.witnesses`.
- `GET /api/witnesses/<endpoint_id>/ledgers?offset&limit`: a proxy of
  the net client's `list` against that witness, addressed exactly the
  way the crawler addresses sources. Answer mirrors the witness's own
  ledger list document: `{ok, ledgers: [{ledger_id, declared_kind,
  head_seq, head_event, event_count, fork_count}], offset, limit,
  more}`, plus `endpoint_id`. Unreachable answers 502 with reason
  `witness_unreachable`.
- `GET /api/resolve/<hostname>`: one TXT lookup of
  `_mabel.<hostname>.` through the ticket 024 resolver seam. Answer
  `{ok, hostname, identity_id | null, status}` with status one of
  `resolved`, `no_record`, `mismatched_records` (records exist, none
  parse), `unreachable`. Never cached; this is navigation, not
  verification.
- `POST /api/identities/<id>/fetch` body `{from: endpoint_id | null}`:
  the CLI `sync fetch` behind a route. `from: null` tries known
  witnesses in the crawler's source order. Answer mirrors the CLI
  fetch document (`stored`, `head_seq`, `controlled_by`).

Removed with the verify tab: `POST /api/verify` and its fixtures. The
CLI verify commands do not use HTTP and are untouched.

Decision: `GET /api/identities/<id>` keeps answering only for stored
ledgers; the identity page composes it with `GET /api/lookup/<id>` and
shows what exists. Decision: the witness drill-in does not auto-fetch
ledgers; fetching is the explicit button on the identity page.
Decision: existing testids survive where the element survives; testids
of removed screens are removed, and the e2e stories are rewritten to
the new screens in the same change.

## Alternatives considered

- Keep the lookup screen and add a link from the wallet: rejected, two
  screens for one identity is the confusion being removed.
- Auto-fetch any ledger the user views: rejected, a page view should
  not write to the home.
- A protocol addition for witness browsing: unnecessary, `List` exists.

## Consequences

Easier: one identity page to maintain and test; the wallet reads as an
address book. Harder: stories 001 to 007 and their specs change with
the screens; the UI verify report steps move to CLI assertions.
