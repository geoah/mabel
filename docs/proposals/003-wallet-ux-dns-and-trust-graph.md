# 003: profiles, DNS verification, the trust graph and the wallet redesign

- Date: 2026-08-24
- Status: proposed
- Decisions affected: implements 014, 015 and 016; extends proposal 002 with
  payload tag 17; adds routes and fixtures to `contracts/`

## Context

Decisions 014, 015 and 016 land three things the current wallet cannot do. The
wallet has no names: identities are 52-character ids plus a local alias that
never leaves the node home, so two people looking at the same identity see
different labels and neither sees what the identity calls itself. There is no
way to tie an identity to a hostname. And trust is visible only one hop out, in
the issuer's own ledger, so "how do I know this person" has no answer. Decision
014 also rejects the current screen layout: stacked label-over-value panels
built for a developer reading a ledger, rather than an address book.

This proposal settles the on-ledger profile payload, the DNS check, the crawl
and its store, the wallet information architecture, and the routes and tickets
that carry them.

## Proposal

### 1. Profiles on the ledger

Decision: **one new payload, `ProfileUpdate`, at tag 17**, not a separate
`HostnameClaim`. A hostname claim is a profile with only the hostname set, and
one payload means one descriptor, one field-table block, one builder and one
latest-wins fold rule instead of two of each. Tags 18 and 19 stay free; 20 to 29
remain reserved for the proposal 002 section 9 deferrals.

```protobuf
message ProfileUpdate {
  string display_name = 1;  // <= 64 bytes UTF-8, no control characters
  string hostname = 2;      // <= 253 bytes, lowercase LDH, at least one dot
}
```

Decision: **no contact fields on the ledger, ever, in this POC.** The owner
listed email as identity metadata, and it must not go here: decision 003 makes
the chain the full history, and proposal 001 section 1 makes ledgers public
replicated data, so an on-ledger address is a permanent publication to anyone
who can name the ledger id, with no delete and no reach-back into the replicas
that already have it. A later `ProfileUpdate` that omits the field changes only
what the fold reports; the bytes stay in every copy forever. The public contact
affordance is the hostname, which is already public DNS and which the person
controls. Private contact details stay local, in `IdentityMeta` beside the local
alias, for both own and foreign identities, and are never signed or synced. A
free-text bio is excluded for the same reason plus the abuse surface it opens.

Fold: latest wins. `LedgerState` gains `profile: Profile { display_name:
Option<String>, hostname: Option<String>, event: EventId, seq: u64 }`, replaced
whole by each `ProfileUpdate`. Any current `CONTROLLER` may append one, under
the uniform rule of proposal 002 section 5, so a delegate can rename the ledger;
that is inherent in delegation and is why verification output names the signing
principal.

Decision: **empty means unset, expressed as absence.** The canonical encoding
forbids serializing a proto3 default, so an unset field is simply not on the
wire, and clearing a name is a `ProfileUpdate` that omits it. A `ProfileUpdate`
with both fields omitted is a legal zero-length payload body under a present
oneof branch, and it means "clear both". Validation: `display_name` rejects C0
and C1 control characters and leading or trailing whitespace; `hostname` rejects
uppercase, a trailing dot, empty labels, labels over 63 bytes and any character
outside `a-z0-9-.`, and requires at least one dot. Both are byte-capped before
any allocation, like every other field.

Names propagate peer to peer because they are on the ledger. The local alias
survives as a private nickname only (proposal 001: the alias is never signed).

### 2. DNS verification

Record shape: a TXT record at `_mabel.<hostname>` whose value is
`mabel=<identity id>`, the id in the same lowercase base32 the rest of the
system displays. Verification passes if **any** TXT record at that label matches
exactly, so one hostname can back several identities, a person and an
organization on the same domain.

Decision: **verification runs on the wallet node only, using
`hickory-resolver`** (0.26.1, tokio, reads the system resolver configuration and
accepts an explicit resolver for tests). The standard library cannot query TXT
records at all, so `getaddrinfo` is not an option. Witnesses do not verify: they
hold no user context, a witness result would have to be trusted to be useful,
and a crawling resolver is a signal witnesses should not emit. The witness UI
shows a claimed hostname as claimed.

Cache, in the node home, one file per identity because a ledger has at most one
current hostname: `verification/<identity_id>.json` holding `{hostname, status,
checked_at_ms, detail}`. It is advisory and rebuildable; deleting the directory
costs one round trip per identity.

Freshness: a cached result is good for 24 hours, the same window decision 016
sets for the graph. Decision: **re-checks are lazy, not scheduled.** Serving an
identity document with an entry older than 24 hours triggers a check and returns
the fresh result; there is no background timer, matching decision 016's "manual
first, periodic later" discipline. `POST /api/identities/:id/verification`
forces one.

Statuses, all advisory and never gating ledger validity (decision 015):
`verified`, `mismatched` (records exist at the label, none carries this id),
`unverified` (no record or NXDOMAIN), `unreachable` (resolver error or the
5-second timeout), `unclaimed` (the ledger claims no hostname). The UI renders a
check for `verified` with the hostname beside it, a warning glyph for
`mismatched`, dimmed text for `unverified` and `unreachable`, and nothing for
`unclaimed`, with the same advisory note the declared kind already carries.

### 3. Trust graph

Crawl: breadth-first from the union of the unrevoked trust subjects of every
identity in this node home. For each frontier identity, fetch the ledger through
the existing multi-source path (configured witnesses, then cached peer hints),
verify it with the fold, record its profile and its own unrevoked subjects, and
enqueue those at the next depth.

Decision: **caps first, completeness never.** Depth defaults to 2 and is bounded
to 1 through 4; at most 500 nodes and 200 fetches per synchronize; 5 seconds per
fetch and 60 seconds for the whole run; anything beyond a cap stops the crawl
and sets a truncation flag rather than failing it. Decision 016 says the graph
stays small, and a bounded partial graph is honest as long as the UI says so.

Store, under `graph/` in the node home:

- `graph/nodes/<identity_id>.json`: declared kind, profile, head seq and event,
  `fetched_at_ms`, `source` (the endpoint that served it), `depth`,
  `discovered_via` (the identity and attestation event that led here), fetch
  status, and the node's own outgoing edges as `{subject, attestation_event,
  seq}`.
- `graph/summary.json`: `last_sync_ms`, depth used, node and edge counts,
  truncation flags, and the identities the crawl started from.

Edges live inside the node that signed them, since an edge is a fact about that
ledger; there is no separate edge file to keep consistent. Reverse edges are
computed by scanning the node files at load, which is trivial at 500 nodes, and
are labelled `best_effort` everywhere they surface: they say who in *my crawl*
trusts this identity, never who trusts them.

Staleness: a node is stale 24 hours after its `fetched_at_ms`, and the graph is
stale 24 hours after `last_sync_ms`. Synchronizing is manual: `mabel graph sync` on
the CLI, `POST /api/graph/sync` on the API and one button in the wallet header,
each returning the counts and the truncation flags.

Lookup answers "how do I know this identity": the shortest path length in edges
(a directly trusted identity is 1), up to three shortest paths rendered as hops
with resolved names, their outgoing trust list, and best-effort reverse edges.
Paths are computed over unrevoked edges as of the last crawl, and every answer
carries `fetched_at_ms` and a `stale` flag.

Decision: **witness-side crawling is specified but deferred.** A witness may
pull ledgers named by trust attestations in ledgers it already stores, recording
in the per-ledger `meta.json` a `pull_reason` of `pushed` or
`referenced_by:<ledger id>`. Admission (proposal 002 section 5) is unaffected
because this is a pull, not a push, but it does mean a witness storing ledgers
that never named it, so the setting defaults to off and the existing global
storage cap bounds it. This is second priority and its ticket may slip without
blocking anything else.

### 4. Wallet information architecture

The wallet route becomes an address book (decision 014). Top to bottom:

- **Identity selector**, a control listing the node's own identities by resolved
  name with the id beside it, remembering the last choice in `localStorage`.
- **Overview**, one compact key-value table, key and value on a line, never
  stacked: name, copyable id, declared kind, created, hostname with its
  verification icon, and counts (events, people trusted, principals, open
  invitations).
- **Ledger**, one line per event as sequence plus event type, each expandable to
  the event detail that the current panels show.
- **State**: the trusted list with resolved names, verification icons and links
  into the lookup view; the principals table, shown only when the ledger has
  more than its root principal.
- **Actions**, each a one-line description of what it does: trust someone,
  revoke trust, set profile, invite, accept, admit, remove, add a witness, push
  to witnesses, verify a claim, synchronize the graph.
- **Developer mode**, a toggle in the header menu, default off, persisted in
  `localStorage` under `mabel.developer_mode`. On, it reveals what the current
  screens show by default: head event ids, witness endpoint ids, principal keys,
  sync freshness, fork provenance and the raw response document. Nothing is
  removed from the product, only moved behind the toggle.

The **lookup view** for a foreign identity, reached from any name, shows the
same overview table plus verification, the trust path from me with each hop
named, their outgoing trust list, and the best-effort reverse list. Each entry
in those lists expands one level in place, and expansion is capped at two levels
so a lookup cannot walk the whole graph.

Decision: **name resolution is profile, then local alias, then truncated id**,
in that order, and the id is always rendered beside the name through the
existing `Identifier` component. A name from a ledger is a claim, not an
identifier: it is not unique, anyone can claim any string, and the UI never
sorts, matches or deduplicates on it. Where a resolved name comes from a crawled
ledger rather than a verified hostname, the tooltip says so.

### 5. API surface and fixtures

Contract-first, per `contracts/README.md`: the fixture lands first, then the
axum handler, the CLI renderer and the UI types in the same change.

Changed: `GET /api/identities/:identity_id` gains `profile {display_name,
hostname, event, seq}` and `verification {status, hostname, checked_at_ms}`.
`GET /api/identities` gains `display_name` and `verification_status` per row so
the selector needs one request.

New: `POST /api/identities/:identity_id/profile` appends a `ProfileUpdate`;
`POST /api/identities/:identity_id/verification` forces a DNS check;
`GET /api/lookup/:identity_id` returns metadata, verification, `degrees`,
`paths`, `trusts`, `trusted_by` with `best_effort: true`, `fetched_at_ms` and
`stale`; `GET /api/graph` returns the summary; `POST /api/graph/sync` runs a
crawl and returns counts, truncation flags and `last_sync_ms`.

New fixtures: `wallet-post-identity-profile.json`,
`wallet-post-identity-verification.json`, `wallet-get-lookup.json`,
`wallet-get-graph.json`, `wallet-post-graph-sync.json`, plus edits to
`wallet-get-identity.json` and `wallet-get-identities.json`. CLI fixtures
`profile-set.json`, `graph-sync.json` and `lookup.json` cover `mabel profile set
--identity <alias|id> [--display-name <n>] [--hostname <h>]`, `mabel graph sync
[--depth N]` and `mabel lookup <identity-id>`. Errors follow the existing
envelope; a lookup for an identity absent from the graph is a 200 with
`degrees: null` and an empty path list, not a 404, because "not in my crawl" is
an answer.

### 6. Migration and ticket cut

Nothing here changes an existing digest, signature domain, transport frame or
storage layout for events. `ProfileUpdate` is an appended payload tag, so
existing golden vectors keep their bytes; the additions are new vectors.

- **023, profile payload**: `ledger.proto` tag 17, the descriptor and field
  table entries, the builder, the latest-wins fold and `LedgerState.profile`,
  golden vectors and rejection vectors for every field rule. Core only, depends
  on nothing in flight.
- **024, DNS verifier**: `hickory-resolver`, the record shape, the five
  statuses, the cache and the 24-hour rule, with a stub resolver in tests and no
  test touching the public internet. Depends on 023.
- **025, trust graph crawler and store**: the BFS, the caps, `graph/`, staleness
  and reverse edges. Depends on 023 for names in nodes and on the existing
  multi-source fetch from ticket 011.
- **026, contracts and routes**: the fixtures of section 5 first, then the
  handlers and the CLI `--json` shapes. Depends on 023, 024 and 025.
- **027, wallet redesign**: the information architecture, developer mode, name
  resolution and the identity selector. Depends on 026. Decision: ticket 019
  (principals and verify screens, unstarted) folds into this one and is closed
  as superseded, because building it on the old layout would be work thrown away
  in the same week.
- **028, lookup and graph view**: the foreign-identity drill-down, path
  rendering and the two-level expansion. Depends on 026 and 027.
- **029, witness crawl provenance**: the deferred piece of section 3, off by
  default, second priority, may slip.

Ticket 021 (membership HTTP routes) is unaffected beyond the identity document
growing two fields; ticket 016 (CLI integration) gains shape assertions for the
new `--json` documents. Tests stay fast: the verifier and crawler take an
injected resolver and an injected fetcher, so their unit tests are pure and the
networked paths keep explicit short timeouts.

## Alternatives considered

- **Separate `ProfileUpdate` and `HostnameClaim` payloads**: two tags, two
  descriptors and two fold rules for one document, with no case where a
  hostname changes independently of the profile it belongs to.
- **Contact fields (email, phone) on the ledger**: the owner named email as
  metadata, but decision 003's history rule makes it unrevocable and public
  forever. Local private notes and a verified hostname cover the need without
  publishing an address to every replica.
- **A `.well-known` HTTPS file instead of TXT**: needs a running web server per
  hostname, drags in a TLS client and a redirect policy, and cannot be checked
  from a resolver-only environment. TXT is one lookup.
- **Signed DNS proofs or on-ledger DNS receipts**: would make an advisory hint
  look like a checked fact, and decision 015 says advisory.
- **Background verification and background crawling**: rejected for now by
  decision 016's "manual first" and because a daemon that dials out on a timer
  is the wrong default for a wallet holding keys.
- **A graph service or shared index**: contradicts decision 006 (no application
  servers) and flag D of the hearsay digest, which names global discovery as an
  explicit hole rather than a feature.
- **Storing edges in one `graph/edges.json`**: a second write path to keep
  consistent with the node files, for no gain at 500 nodes.
- **Keeping the current wallet layout and adding panels**: decision 014 rejects
  stacked label-over-value lists outright, and each new panel makes the eventual
  redesign larger.

## Consequences

Easier: identities have names that travel with them, so the address book reads
as an address book and two wallets agree on what an identity calls itself. A
hostname plus a TXT record gives a person a recognisable handle with no registry
and no server. Degrees of separation turn a bare id into "Theo trusts Alice,
Alice trusts Bob", which is the product claim decision 016 asks for. Developer
mode keeps every existing panel available without putting it in a user's way.

Harder: names are claims, so every surface that shows one has to show the id and
resist the temptation to match on the name; impersonation is now possible in a
way it was not when only ids existed. The wallet gains an outbound DNS resolver
and a crawler, both new failure modes on a machine that holds keys, both bounded
by caps and short timeouts and both off unless asked. The graph is partial by
construction, so "who trusts them" is always best-effort and must be labelled
that way every single time.

Deferred: witness-side crawling and its pull-reason provenance, background
verification and background sync, any contact field on the ledger, and any
notion of a unique or registered name.
