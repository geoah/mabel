# 003: profiles, DNS verification, the trust graph and the wallet redesign

- Date: 2026-08-24
- Status: accepted (2026-08-25, after dual review by Codex and an independent
  Opus reviewer; 13 merged revision items applied)
- Decisions affected: implements 014, 015 and 016; extends proposal 002 with
  payload tag 17; adds routes and fixtures to `contracts/` and amends its
  payload-table freeze

## Context

Decisions 014, 015 and 016 land three things the current wallet cannot do. The
wallet has no names: identities are 52-character ids plus a local alias that
never leaves the node home, so two people looking at the same identity see
different labels and neither sees what the identity calls itself. There is no
way to tie an identity to a hostname. Trust is visible only one hop out, in the
issuer's own ledger, so "how do I know this person" has no answer. And decision
014 rejects the current screen layout, stacked label-over-value panels built for
a developer reading a ledger rather than an address book. This proposal settles
the profile payload, the DNS check, the crawl and its store, the information
architecture, and the routes and tickets that carry them.

## Proposal

### 1. Profiles on the ledger

Decision: **one new payload, `ProfileUpdate`, at tag 17**, not a separate
`HostnameClaim`. A hostname claim is a profile with only the hostname set, and
one payload means one descriptor, one field-table block, one builder and one
latest-wins fold rule instead of two of each. Tags 18 and 19 stay free; 20 to 29
remain reserved for the proposal 002 section 9 deferrals.

```protobuf
message ProfileUpdate {
  string display_name = 1;  // <= 64 bytes UTF-8; absent means unset
  string hostname = 2;      // <= 246 bytes, lowercase LDH; absent means unset
}
```

Decision: **no contact fields on the ledger, ever, in this POC.** The owner
listed email as identity metadata, and it must not go here: decision 003 makes
the chain the full history and proposal 001 section 1 makes ledgers public
replicated data, so an on-ledger address is a permanent publication to anyone
who can name the ledger id, with no delete and no reach-back into replicas. A
later `ProfileUpdate` that omits the field changes only what the fold reports;
the bytes stay in every copy forever. The public contact affordance is the
hostname, already public DNS and controlled by the person; everything else goes
in the local contact store below. A free-text bio is excluded for the same
reason plus the abuse surface it opens.

Fold: latest wins, whole document. `LedgerState` gains `profile: Profile {
display_name: Option<String>, hostname: Option<String>, event: EventId, seq:
u64, signing_principal: (IdentityId, PublicKey) }`, replaced entire by each
`ProfileUpdate`. Any current `CONTROLLER` may append one, under the uniform rule
of proposal 002 section 5, so a delegate can rename the ledger; recording the
signing principal on the profile is what keeps that visible.

Decision: **the operation is replacement, not patch, and every surface says
so.** The CLI is `mabel profile replace --identity <alias|id> --display-name
<name> [--hostname <host>]`, an omitted flag **clears** that field, and the
command prints a before-and-after diff and asks for confirmation unless `--yes`
is given. The HTTP body requires both keys and allows either to be null, so no
client can half-specify a replacement. A patch verb is not offered, because a
partial update over a whole-document payload is the shape that silently drops a
hostname. A `ProfileUpdate` whose effect equals the current folded profile is
refused before signing with `no_op_profile_update`, a node guard rather than a
fold rule, since the fold must accept whatever a valid chain contains.

Two controllers replacing the profile at once is the ordinary shared-ledger
race, resolved by the append discipline of proposal 001 section 5: query the
witnesses' heads, fast-forward, re-sign on the new head, exit 50 on stale state.
The last event on the branch wins and the loser sees the diff again.

Decision: **empty means unset, expressed as absence.** The canonical encoding
forbids serializing a proto3 default, so an unset field is simply not on the
wire, clearing a name is a `ProfileUpdate` that omits it, and an explicitly
encoded empty string is already rejected as `DefaultValueEncoded`. A
`ProfileUpdate` with both fields omitted is a legal zero-length payload body
under a present oneof branch and means "clear both"; it gets its own golden
vector.

Decision: **the wire validator learns strings.** A new `FieldKind::String {
max }` checks the byte cap, decodes UTF-8 from the scanned slice without
allocating, and rejects invalid UTF-8, C0 and C1 controls, the bidi controls
`U+202A..U+202E` and `U+2066..U+2069`, zero-width and invisible format
characters (`U+200B..U+200F`, `U+2060..U+2064`, `U+FEFF`), leading or trailing
whitespace, and, for `display_name`, any value that parses as a valid identity
id, so a name can never masquerade as an identifier. New `WireError` codes:
`invalid_utf8`, `invalid_display_name`, `invalid_hostname`. Hostname syntax is
normative in section 2, checked by the same descriptor.

Decision: **private contact metadata lives in `contacts/<identity_id>.json`**,
one file per identity, holding `{nickname, note, updated_at_ms}` with `nickname`
capped at 64 bytes and `note` at 512, both UTF-8 under the codepoint rules
above. It covers foreign identities as well as this node's own, is never signed
or synced, and is what decision 014 means by contact metadata. It is
deliberately not part of `IdentityMeta`, which describes identities this node
controls and is `deny_unknown_fields`. Names themselves propagate peer to peer
because they are on the ledger; the local alias survives as a private nickname
for this node's own identities (proposal 001: the alias is never signed).

### 2. DNS verification

Record shape: a TXT record at `_mabel.<hostname>` whose value is
`mabel=<identity id>`, the id in the same lowercase base32 the rest of the
system displays. One hostname can back several identities, a person and an
organization on the same domain.

Decision: **verification runs on the wallet node only, using
`hickory-resolver`** (0.26.1, tokio) behind an injectable `Resolver` trait whose
single method resolves TXT records for an absolute name, so unit tests use a
stub and no test touches the public internet; the standard library cannot query
TXT at all. Witnesses do not verify: they hold no user context, a witness result
would have to be trusted to be useful, and a crawling resolver is a signal
witnesses should not emit. The witness UI shows a claimed hostname as claimed.

Decision: **query construction and matching are normative.**

- The query name is built absolute, `_mabel.<hostname>.` with the root label
  appended, and the resolver's search list is disabled, so no local suffix can
  be appended to a claim.
- `hostname` is at most 246 bytes (253 minus `_mabel.`), ASCII only, one to 63
  bytes per label, each label starting and ending alphanumeric with interior
  characters from `[a-z0-9-]`, at least one dot, no trailing dot, no uppercase.
- Within one TXT resource record, character-strings are concatenated with no
  separator; strings are never concatenated across records.
- A record matches when its concatenated value begins with `mabel=`, the prefix
  compared case-insensitively, and the remainder parses under the existing
  case-insensitive id codec to this identity id.
- No `mabel=` record at the label is `unverified`. One or more `mabel=` records
  that do not carry this id is `mismatched`.
- CNAME is followed to at most four links; a loop, a longer chain, a timeout or
  any resolver error is `unreachable`.

Cache, one file per identity in the node home:
`verification/<identity_id>.json` holding `{hostname, status, checked_at_ms,
last_verified_at_ms, detail}`. It is advisory and rebuildable.

Decision: **the entry is bound to the hostname it verified.** If the ledger's
current `profile.hostname` differs from the cached `hostname`, the entry is
treated as absent, so a renamed claim can never inherit the old verdict.

Decision: **a failed re-check never overwrites a decisive result.** `verified`
and `mismatched` are decisive and persist with their `checked_at_ms`; an
`unreachable` re-check is recorded beside the decisive result with its own
timestamp, and the document reports both. A `verified` result older than 24
hours is served with `stale: true` and is never rendered as a plain check.

Decision: **lazy re-checks happen on the single-identity GET only.** That route
answers from cache immediately, including `checked_at_ms`, `stale` and
`last_verified_at_ms`, and starts at most one background refresh per identity
when the entry is stale. `GET /api/identities` is cache-only and never triggers
a lookup, so listing a hundred identities cannot fan out into a hundred queries.
`POST /api/identities/:identity_id/verification` forces a check and waits for
it. There is no background timer, matching decision 016's "manual first,
periodic later" discipline.

Statuses, all advisory and never gating ledger validity (decision 015):
`verified`, `mismatched`, `unverified`, `unreachable` and `unclaimed`. The UI
renders a check for fresh `verified` with the hostname beside it, a check with a
stale marker for an aged one, a warning glyph for `mismatched`, dimmed text for
`unverified` and `unreachable`, nothing for `unclaimed`, and the same advisory
note the declared kind already carries.

### 3. Trust graph

Decision: **every local identity is a crawl root at depth 0** and its root
provenance is retained on each discovered node, so the graph answers from any of
this wallet's identities and a subject trusted directly by a root is at depth 1.

Decision: **source order per frontier ledger is normative**, in this order, with
every applicable source queried rather than stopping at the first:

1. a local copy under `ledgers/`, for ledgers this node already holds;
2. `peers.json` hints recorded for that ledger id;
3. the node-wide witnesses configured in `node.json`;
4. witnesses named by a verified copy of that ledger's own `WitnessConfig`,
   which is reachable only after one of the first three produced a copy.

Heads from the queried sources are compared under the existing equivocation rule
(proposal 001 section 3.7): two valid candidates that diverge at a sequence are
recorded on the graph node with both source endpoints and both event ids, shown
in lookup, and never silently resolved to one branch. A source that served a
verified copy is written back to `peers.json` as a hint for next time.

Decision: **the crawler verifies in memory and writes no stranger's ledger.** It
uses the `WalletSync::candidate` path, folds the candidate and keeps only a
derived summary; `ledgers/` stays the store for identities this node controls or
fetched deliberately, because a crawl that populated it would turn every wallet
into a replica and blur which ledgers the user is responsible for. Ticket 025
defines the `LedgerFetcher` trait wrapping this source order, so the crawler's
tests inject a fake.

Decision: **caps first, completeness never.** Depth defaults to 2 and is bounded
to 1 through 4; at most 500 nodes; at most 8 fetches in flight per level; 5
seconds per fetch; 60 seconds for the whole run, authoritative. The fetch cap
survives at 300 with a reason: the wall clock bounds the slow case (8 in flight
at 5 seconds each cannot exceed roughly 96 fetches in 60 seconds) but not the
fast one, where sub-100-millisecond responses would let one sync hammer a
witness with thousands of requests. Node count bounds the graph, the clock
bounds a slow crawl, the fetch cap bounds a fast one. Crawl order is
breadth-first with ties broken by ascending identity id, so a truncated crawl is
deterministic and reproducible.

Decision: **each sync writes a new generation and swaps a pointer.** A sync
writes `graph/generations/<sync_id>/`, one file per node plus `summary.json`,
then atomically replaces `graph/current.json`, the pointer naming the live
generation. Readers resolve the pointer once and read only that generation, so
no lookup sees a half-written crawl; older generations are caches, garbage
collected down to the last two. `sync_id` is the start timestamp plus a random
suffix.

- `graph/generations/<sync_id>/nodes/<identity_id>.json`: declared kind,
  profile, head seq and event, `fetched_at_ms`, `source` (the endpoint that
  served it), `depth`, `roots` (which local identities reach it and at what
  depth), `discovered_via` (identity plus attestation event), fetch status, any
  recorded equivocation, and the node's outgoing edges as `{subject,
  attestation_event, seq}`.
- `graph/generations/<sync_id>/summary.json`: `sync_id`, `last_sync_ms`, depth
  used, roots, node and edge counts, `truncated`, and `truncated_by` in
  `depth | nodes | fetches | time`.

Edges live inside the node that signed them, since an edge is a fact about that
ledger, and reverse edges are computed by scanning the generation at load, which
is trivial at 500 nodes. Reverse edges are always labelled: the response object
is `{best_effort: true, entries: [...]}`, and they report who in this crawl
trusts an identity, never who trusts them.

Staleness: a node is stale 24 hours after its `fetched_at_ms`, the graph 24
hours after `last_sync_ms`. Synchronizing is manual: `mabel graph sync` on the
CLI, `POST /api/graph/sync` on the API and one button in the wallet header, each
returning counts, `truncated` and `truncated_by`.

Lookup answers "how do I know this identity" **relative to one root**:
`GET /api/lookup/:identity_id?from=<identity_id>`, defaulting to the identity
currently selected in the wallet, and `mabel lookup <id> --from <alias|id>` on
the CLI. It returns the shortest path length in edges from that root, up to
three shortest paths rendered as hops, the target's outgoing trust list, and the
best-effort reverse list. Every response carries `graph_stale`,
`graph_truncated`, `truncated_by`, and per-hop `fetched_at_ms` and `stale`.
`degrees: null` means no path was found **within the caps**, which the UI states
as "shortest path found in this crawl", never as "no relationship". Equivocation
recorded on any node in a path is shown on that hop.

Decision: **witness-side crawling is specified but deferred.** A witness may
pull ledgers named by trust attestations in ledgers it already stores, recording
in the per-ledger `meta.json` a `pull_reason` of `pushed` or
`referenced_by:<ledger id>`. Admission (proposal 002 section 5) is unaffected
because this is a pull, not a push, but it does mean a witness storing ledgers
that never named it, so the setting defaults to off and the existing global
storage cap bounds it. Second priority; its ticket may slip.

### 4. Wallet information architecture

The wallet route becomes an address book (decision 014). Top to bottom:

- **Identity selector**, listing this node's identities as resolved names with
  ids beside them, remembering the last choice in `localStorage`. The selection
  is also the default `from` for lookups.
- **Overview**, one compact key-value table, key and value on a line, never
  stacked: name, copyable id, declared kind, created, hostname with its
  verification icon, contact (the local nickname and note), and counts (events,
  people trusted, principals, open invitations).
- **Ledger**, one line per event as sequence plus event type, each expandable to
  the event detail the current panels show.
- **State**: the trusted list with resolved names, verification icons and links
  into lookup; the principals table, shown only when the ledger has more than
  its root principal.
- **Actions**, each with a one-line description: trust someone, revoke trust,
  replace profile, edit contact, invite, accept, admit, remove, add a witness,
  push to witnesses, verify a claim, synchronize the graph.
- **Developer mode**, a toggle in the header menu, default off, persisted in
  `localStorage` under `mabel.developer_mode`. On, it reveals what the current
  screens show by default: head event ids, witness endpoint ids, principal keys,
  sync freshness, fork and crawl provenance, and the raw response document.
  Nothing is removed from the product, only moved behind the toggle.

The **lookup view** for a foreign identity shows the same overview table plus
verification, the path from the selected root with each hop named, their
outgoing trust list, and the best-effort reverse list. Entries expand one level
in place, capped at two levels so a lookup cannot walk the whole graph.

Decision: **one `ResolvedIdentity` object renders every foreign identity.** The
contract object is `{identity_id, display_name, alias, hostname, verification,
provenance}`, where `provenance` is `profile`, `alias` or `none`, and it is what
the API returns in selector rows, trusted lists, path hops, lookup headings,
expansions and reverse edges. The UI gets one `ResolvedIdentity` component; the
existing `Identifier` component has no name slot today and gains one. Resolution
order is profile display name, then local alias or contact nickname, then the
truncated id.

Anti-spoofing rules the component enforces so no screen can forget them: a name
never renders in the same style as an id or a hostname (names are plain text,
ids and hostnames monospace with the copy control), so a display name of
"alice.example" cannot pass as a verified hostname; the id is always beside the
name; two entries in one list resolving to the same name both show their full
ids instead of the truncation; and the UI never sorts, matches or deduplicates
on a name. Consent: before the first hostname publication and before the first
graph sync, the wallet shows a short panel stating what becomes public or
observable (the sentences in Consequences) and requires explicit confirmation,
remembered per node home.

### 5. API surface and fixtures

Contract-first, per `contracts/README.md`: the fixture lands first, then the
axum handler, the CLI renderer and the UI types in the same change.

Decision: **both identity routes return the same document.** `GET
/api/identities` rows and `GET /api/identities/:identity_id` share one shape,
including the nested objects, with explicit nulls rather than omitted keys, so
the UI has one type and one renderer. The document gains:

- `profile`: `{display_name, hostname, event, seq, signing_principal {identity,
  key}}` or null;
- `verification`: `{status, hostname, checked_at_ms, last_verified_at_ms, stale,
  detail}`, with `status: "unclaimed"` when the profile names no hostname;
- `contact`: `{nickname, note, updated_at_ms}` or null.

New routes: `POST /api/identities/:identity_id/profile`, whose body requires
both keys and accepts nulls, appending a `ProfileUpdate` and answering 409
`no_op_profile_update` when nothing would change; `POST
/api/identities/:identity_id/verification`, which forces a check and waits; `GET`
and `PUT /api/identities/:identity_id/contact` for the local store, valid for
foreign ids too; `GET /api/lookup/:identity_id?from=<identity_id>`; `GET
/api/graph`; `POST /api/graph/sync`.

New fixtures: `wallet-post-identity-profile.json`,
`wallet-post-identity-verification.json`, `wallet-get-identity-contact.json`,
`wallet-put-identity-contact.json`, `wallet-get-lookup.json`,
`wallet-get-graph.json` and `wallet-post-graph-sync.json`, plus edits to
`wallet-get-identity.json` and `wallet-get-identities.json`. CLI fixtures
`profile-replace.json`, `contact-set.json`, `graph-sync.json` and `lookup.json`
cover `mabel profile replace`, `mabel contact set`, `mabel graph sync` and
`mabel lookup <id> --from <alias|id>`. A lookup for an identity absent from the
graph is a 200 with `degrees: null` and an empty path list, not a 404, because
"not in my crawl" is an answer.

`contracts/README.md` edits: index rows for each new fixture; the payload table
gains `profile_update` with keys `display_name` and `hostname`; CLI rows for the
four new commands; a nullability note stating that identity documents carry
explicit nulls; and a line recording that this proposal amends the payload-table
freeze, since that table was frozen before tag 17 existed.

### 6. Migration and ticket cut

Nothing here changes an existing digest, signature domain, transport frame or
event storage layout. `ProfileUpdate` is an appended payload tag, so existing
golden vectors keep their bytes; the additions are new vectors.

| Ticket | Scope | Depends on |
|---|---|---|
| 023 profile payload | tag 17, `FieldKind::String` and the three `WireError` codes, descriptor and field table, builder, latest-wins fold with `signing_principal`, the node-side no-op guard, golden and rejection vectors including the zero-length payload | nothing in flight |
| 024 DNS verifier | the `Resolver` trait, `hickory-resolver`, the normative query and matching rules, the five statuses, hostname binding, decisive-result retention, the cache and the 24-hour rule, stub-resolver tests | 023 |
| 025 crawler and store | the `LedgerFetcher` trait and source order, in-memory verification, BFS with the caps and concurrency, generations plus `current.json`, staleness, reverse edges, equivocation recording | 023, existing 011 fetch |
| 026 contracts and routes | fixtures first, then handlers and CLI `--json`: the shared identity document, `ResolvedIdentity`, the contact store and its routes, lookup with `from` | 023, 024, 025 |
| 027 wallet shell | identity selector, name resolution, developer mode, the `Identifier` name slot and the `ResolvedIdentity` component, mock-store and UI test updates | 026 |
| 028 identity view | overview table, ledger lines, state, actions, absorbing ticket 019's membership screens | 026, 027, and ticket 021's membership fixtures |
| 029 lookup and graph view | foreign-identity drill-down, path rendering, two-level expansion, staleness and truncation surfaces | 026, 027 |
| 030 witness crawl provenance | the deferred piece of section 3, off by default, may slip | 025 |

Decision: ticket 019 (principals and verify screens, unstarted) folds into 028
and is closed as superseded; building it on the old layout would be work thrown
away in the same week. Ticket 021 (membership HTTP routes) keeps its fixtures
and is a dependency of 028 rather than a casualty. Ticket 016 (CLI integration)
gains shape assertions for the new `--json` documents. Tests stay fast because
the verifier takes a `Resolver` and the crawler a `LedgerFetcher`, so their unit
tests are pure and the networked paths keep short explicit timeouts.

## Alternatives considered

- **Separate `ProfileUpdate` and `HostnameClaim` payloads**: two tags, two
  descriptors and two fold rules for one document, with no case where a hostname
  changes independently of the profile it belongs to. **A patch verb**: partial
  updates over a whole-document payload are how a hostname disappears unnoticed.
- **Contact fields on the ledger**: the owner named email as metadata, but
  decision 003's history rule makes it unrevocable and public forever. The local
  contact store plus a verified hostname covers the need.
- **A `.well-known` HTTPS file instead of TXT**: needs a web server per
  hostname, a TLS client and a redirect policy, and cannot be checked from a
  resolver-only environment. **Signed DNS proofs or on-ledger receipts**: would
  make an advisory hint look like a checked fact, and decision 015 says
  advisory.
- **Re-checking DNS on the list route**: one page view becomes one lookup per
  row, both slow and a broadcast of the whole address book.
- **Writing crawled ledgers under `ledgers/`**: turns every wallet into an
  unasked-for replica and destroys the distinction between ledgers the user is
  responsible for and ledgers they merely looked at. **One mutable `graph/`
  directory**: a lookup during a sync reads a torn graph, where generations plus
  a pointer swap cost one extra directory.
- **Background verification and background crawling**: rejected for now by
  decision 016's "manual first" and because a daemon that dials out on a timer
  is the wrong default for a wallet holding keys. **A graph service or shared
  index**: contradicts decision 006 and flag D of the hearsay digest, which
  names global discovery as an explicit hole rather than a feature.
- **Keeping the current wallet layout and adding panels**: decision 014 rejects
  stacked label-over-value lists outright, and each new panel makes the eventual
  redesign larger.

## Consequences

Easier: identities have names that travel with them, so the address book reads
as an address book and two wallets agree on what an identity calls itself. A
hostname plus a TXT record gives a person a recognisable handle with no registry
and no server. Degrees of separation turn a bare id into "Theo trusts Alice,
Alice trusts Bob", the product claim decision 016 asks for. Developer mode keeps
every existing panel available without putting it in a user's way.

Harder: names are claims, so every surface showing one must show the id and
resist matching on the name, and impersonation is possible in a way it was not
when only ids existed, which is why one component owns rendering. The wallet
gains an outbound resolver and a crawler, both new failure modes on a machine
holding keys, both bounded by caps and short timeouts and both off unless asked.
The graph is partial by construction, so "who trusts them" is always best-effort
and labelled that way every time.

Privacy, stated plainly and shown to the user before the fact: every display
name and hostname ever set stays readable forever by anyone who can name the
ledger id, because the chain is the full history and replicas keep their copies.
A graph sync tells each contacted witness which identities this wallet cares
about, and the system resolver learns every hostname the wallet checks.

Deferred: witness-side crawling and its pull-reason provenance, background
verification and background sync, any contact field on the ledger, and any
notion of a unique or registered name.
