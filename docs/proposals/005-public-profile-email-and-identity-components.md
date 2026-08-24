# 005: a public email, creation with a profile, and the two identity components

- Date: 2026-08-25
- Status: accepted (owner direction, 2026-08-25)
- Decisions affected: extends the profile of proposal 003 section 2;
  amends proposal 004's screens; decision 017 governs all wording

## Context

The owner wants an identity to carry optional public contact facts (a
name and an email) on its own ledger from the moment it is created,
wants the local alias clearly marked as private, and wants the UI to
render every identity through exactly two reusable components on a
mobile-first single column.

## Proposal

Ledger and API:

- `ProfileUpdate` (payload tag 17) gains `string email = 3`, at most
  254 bytes, UTF-8, no control or bidi codepoints, exactly one `@`
  with at least one byte on each side. No deliverability check: it is
  a claim, like everything else on a ledger. Absent means unset, and
  the replacement semantics stay whole: one event replaces all three
  fields at once. Fold, scanner rule, golden and rejection vectors,
  and every profile and identity fixture follow.
- Identity creation takes an optional public name and email. CLI:
  `mabel identity create --name <display name> --email <email>`; the
  create route accepts `display_name` and `email`. When either is
  given the node appends one `ProfileUpdate` at seq 1 immediately
  after the inception, so a new identity's first two events are who it
  is and what it shows the world. `profile replace` gains `--email`.
- The local alias never leaves the home. The UI names it the
  private nickname and says only this device sees it.

The two identity components, used everywhere without exception:

1. **The inline identity**: one line with the display name, the
   verified-host mark, the pill, and a copy button for the id. Used
   inside sentences, table rows and tight lists.
2. **The identity card**: three or four lines with the name, the id,
   the pill, the public email when known, the kind and a last-seen or
   head fact, plus an expand control that opens the fuller block in
   place. The expanded card is exactly the identity page's top
   section: one component, three states.

The pill on both: `your identity` for an identity this wallet signs
for; green `trusted` when any local identity has an unrevoked
attestation for them; orange `trusted (Nd)` when the graph knows a
shortest path of N degrees; no pill when nothing is known. Decision:
degree comes from the stored crawl only, never a live network call at
render time.

Screen changes:

- Every page is a single column at a fixed readable width on all
  screens; desktop gets margins, not more columns.
- The ledger section is titled Ledger, drawn as compact rows rather
  than a table, with a real pagination footer (previous, next, and
  where you are).
- Trust rows drop positions: `trusted`, or `taken back`.
- The principals section links each identity with the inline
  component; no bare counts. `Invitations waiting` becomes
  `Invitations to help control this identity, not yet answered`.
- Removed outright: the back link element, the declared-kind advisory
  sentence, the DNS advisory sentence, and the key-facts sentence.

## Alternatives considered

- Email inside the inception event: rejected, the profile event
  already owns replaceable public facts and inception stays minimal.
- A separate contacts payload: rejected, one profile event with whole
  replacement is simpler and already shipped.

## Consequences

Easier: one identity rendering to maintain; a new identity is
presentable from birth. Harder: a proto field ripples through the
scanner, fold, vectors, contracts, CLI, UI and the stories again; the
degree pill is only as fresh as the last sync, which the card states.
