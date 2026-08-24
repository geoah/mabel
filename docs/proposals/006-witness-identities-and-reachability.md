# 006: witnesses are identities, reachability is on the ledger

- Date: 2026-08-25
- Status: accepted (2026-08-24, after dual review by Codex and an
  independent Opus reviewer; 25 merged findings applied in revision)
- Decisions amended: **001** (the deliverables name a witness node with its own
  debug UI; section 8 leaves one node program and one UI), **005** (a witness is
  an identity, not an endpoint), **015** (a second recognised TXT key beside
  `mabel=`), **018** (`mabel wallet serve` and `mabel witness run` become
  `mabel serve`, carrying the same `--allow-host` rule unchanged)
- Also: extends proposal 002 with payload tags 18 and 19 and retires the write
  path of tag 11; supersedes the source order of proposal 003 section 3 and the
  witness screens and routes of proposal 004; amends the payload-table freeze in
  `contracts/README.md` a third time
- On acceptance this writes **decision 019, "a witness is an identity and
  reachability is published on the ledger"**, holding the four rules that
  outlive the proposal: a witness is named by identity id, an identity publishes
  the endpoints that answer for it, a home witnesses only for the identities
  `node.json.witness_for` names, and one node program serves one router.
  Decisions 005, 015 and 018 each gain a line pointing at 019.

## Context

A witness is named by a raw Iroh endpoint id. `WitnessConfig.witnesses` holds
32-byte `EndpointId`s, `node.json.witnesses` holds them too, `mabel witness add
--endpoint` takes one, and the UI draws a witness card whose only fact is that
number. Three costs follow. A witness cannot move: its endpoint id is the public
half of its `node.key`, so replacing a machine leaves every ledger event that
named it pointing at nothing, and each of those events is signed and permanent
and can only be superseded by another event on every ledger that named it. A
witness has no name, no profile, no hostname and no trust edges, because it is
not an identity, so the one thing the wallet cannot render as an identity card
is the thing it dials most. And an identity that is not a witness has no
published way to be reached at all: the only reachability facts in the system
are `peers.json`, a `--peer` ticket and someone else's `WitnessConfig`.

The owner's ruling: a witness is a Mabel identity; a ledger event publishes the
endpoints that answer for an identity; DNS and a shareable `mabel://` link carry
those endpoints too; and the witness API and the wallet API become one node API,
because witnessing is a capability of a node and not a different program.

## Proposal

### 1. A witness is an identity

Decision: **payload tag 19, `WitnessSet`, names identity ids, and tag 11
`WitnessConfig` is retired for writing and kept readable forever.**

```protobuf
message WitnessSet {
  repeated bytes witnesses = 1;  // 0..=16 distinct 32-byte identity ids
}
```

Tag 11 is not redefined in place. Three reasons, in order of weight.

The one-time exception is spent. Proposal 002 section 7 rewrote `v0` in place
and wrote its own expiry: "the exception is available once and expires with the
first ledger created outside the test suite". It was available once, proposal
002 took it, and the compose topology and the demo home have both created
ledgers since. There is no second exception to claim, and inventing one would
make the append-only rule of proposal 001 section 3.1 a preference.

An endpoint id and an identity id are both 32 opaque bytes, so this is the one
redefinition a decoder cannot catch. Every other kind of in-place change fails
loudly: a renamed field is an unknown field number, a retyped field is a wire
type mismatch. Here a chain written last week folds cleanly and reports a
witness set of identity ids that are really endpoint ids, admission compares a
witness identity against that list and refuses every push, and resolution tries
to fetch ledgers whose ids are node keys. The failure is a confusing refusal
three layers away from its cause. Ledger ids commit to inception bytes, so those
chains cannot be rewritten either, only reinterpreted.

Keeping tag 11 readable costs one descriptor and one folded field, and buys
something: the retired list means what it always physically was, endpoints that
may hold this ledger. Section 5 reads it as a dial hint under its own source
name and section 4 reads it as a gated legacy admission clause, so an existing
ledger keeps working with no event appended.

Decision: **the fold accepts tag 11 forever; the node refuses to build one.**
The fold must accept whatever a valid chain contains, the same rule proposal 003
gave the no-op profile update. `build_witness_config` stops being callable from
any route, command or UI action; it survives in `sign.rs` behind a test-only
gate, because the golden and rejection vectors for tag 11 are generated from it
and must keep their exact bytes. No shipped code path reaches it.

Decision: **`WitnessSet.witnesses` may be empty, where tag 11 required at least
one.** "Nobody keeps my chain any more" has to be sayable, and section 4 makes
it mean that: an empty set stops later extensions from being admitted anywhere,
rather than leaving the last-named witnesses holding the chain forever. The
minimum of one in tag 11 was an accident of it being the only reachability
statement in the system. An empty repeated field serializes to nothing under the
canonical encoding, so an empty `WitnessSet` is a zero-length payload body under
a present oneof branch, exactly the shape
`test-vectors/14-profile-update-cleared.json` already pins.

Decision: **a ledger may name itself in its own `WitnessSet`.** That is how a
self-hosted identity says "I keep my own chain", and section 5 resolves it to
the identity's own advertised endpoints through the visited-identity set, with
no special case.

### 2. Reachability on the ledger

Decision: **payload tag 18, `EndpointAdvertisement`, publishes the endpoints
that answer for this identity.**

```protobuf
message EndpointAdvertisement {
  repeated bytes endpoints = 1;  // 0..=8 distinct 32-byte Iroh endpoint ids
}
```

The event is on the identity's own chain, signed by one of its controllers, so
reachability becomes a published, replaceable, self-authenticating fact rather
than a hint someone hands you. It is legal on every ledger, not only on a
witness's, which is what makes a person directly fetchable with no witness at
all: a wallet already serves reads over the sync protocol
(`crates/mabel-node/src/wallet/store.rs`), so an advertisement is the only
missing piece.

Decision: **one name on every wire surface, `endpoints`.** The protobuf field,
the JSON key, the DNS key `mabel-endpoints=`, the link parameter `?endpoints=`
and the CLI flag `--endpoints` all use that word. "Nodes", "machines" and
"Iroh ID" are not second spellings of it: `node` already names the home in
`node.json` and `mabel node id`, and decision 012 forbids one thing with two
names. The human label stays **machines**, which is what the UI calls them in
the sentence the reader sees; `endpoints` never appears in UI copy (decision
017).

Decision: **whole replacement, not append.** One event says "these and only
these". Append semantics would need a removal payload, because an endpoint you
appended cannot be unsaid, and a reachability list that can only grow is a list
that can only get more wrong: a stale endpoint costs a dial that times out, and
an endpoint whose `node.key` was regenerated onto another machine is an endpoint
someone else now controls. Replacement also gives one fold rule shape across
three payloads, the profile, the witness set and the advertisement. The chain
still holds every endpoint ever advertised, forever, because the chain is the
full history (decision 003): the history is append-only, the state is replaced.

Decision: **the cap is 8, and the dial cap is separate.** Eight covers a small
fleet behind one identity and a rolling replacement, where a new machine is
advertised before the old one is dropped. It is not the number that bounds
network cost: 16 witnesses each advertising 8 endpoints is 128 endpoints for one
ledger, so section 5 caps what one operation actually dials at 16.

Decision: **an empty advertisement is legal and means "nothing answers for me
right now".** Same shape and same reason as the empty `WitnessSet`, and it is
what an operator appends before decommissioning a machine.

### 3. Field table and fold

Rows added to the table of proposal 002 section 8. Byte lengths are exact.

| Field | Presence | Bytes | Rule |
|---|---|---|---|
| `WitnessSet.witnesses` | 0 to 16 | 32 each | all distinct; each is an identity id, a digest and not a key, so no point check applies; the ledger's own id is allowed |
| `EndpointAdvertisement.endpoints` | 0 to 8 | 32 each | all distinct; each must decompress to a valid ed25519 point, since an endpoint id that is not a point can never be dialled, which is the check tag 11 already applies |

`WITNESS_CONFIG` keeps its descriptor and its rules unchanged. `EVENT_BODY`
gains variants 18 `endpoint_advertisement` and 19 `witness_set`, `MAX_WITNESSES`
is reused for tag 19, and a new `MAX_ENDPOINTS` of 8 covers tag 18. New
`WireError` codes: none. The largest realistic event is still a 16-entry list at
roughly 0.7 KiB, well inside the 4 KiB event cap.

Decision: **this proposal spends the last two free payload tags.** Proposal 003
held 18 and 19 free and 20 to 29 reserved for the deferrals of proposal 002
section 9. After this, the next payload takes a number out of that reserved
block, which is what "reserved" there means: held for named features, not
unusable.

Fold. `state.witnesses()` disappears, because its element type changes and a
silently retyped accessor is a compile that should have failed. Three accessors
replace it:

- `witness_identities() -> &[IdentityId]`, the latest `WitnessSet`, plus
  `witness_set: Option<WitnessSet { witnesses, event, seq, signing_principal }>`
  for the surfaces that report who said it;
- `witness_endpoints() -> &[EndpointId]`, the latest tag 11 `WitnessConfig`,
  empty on a chain that carries none;
- `endpoints() -> &[EndpointId]`, plus `endpoint_advertisement: Option<
  EndpointAdvertisement { endpoints, event, seq, signing_principal }>`.

The two witness fields never overwrite each other: they come from different
payloads and are folded independently, whatever order the events appear in.
Recording the signing principal on tags 18 and 19 matters for the same reason
proposal 003 records it on the profile, and more: re-pointing where an identity
answers redirects everyone who resolves it, and naming a witness set says who
may keep the chain. Both are acts a delegate can perform, and every surface
shows which principal performed them.

### 4. Admission

Decision: **a witness admits an extension while the witness set still names an
identity it witnesses for, and refuses once neither the stored state nor the
pushed state names one.** For a push of chain `C` for ledger `L` arriving at a
home whose `witness_for` set is `W`, let `pre` be the folded state of the copy
this home already stores for `L`, empty if it stores none, and `post` the folded
state of `C`. Admit when, in order:

1. this home holds a signing key for `L`, so it controls the ledger; or
2. `pre.witness_identities()` intersects `W`; or
3. `post.witness_identities()` intersects `W`; or
4. the legacy clause of the next decision holds;

otherwise `Rejected { NOT_ADMITTED }`. Reads stay open to all.

Clause 2 is what admits the removal event itself. A controller who appends a
`WitnessSet` that drops this witness needs that event to reach the witness, or
the witness would keep serving a chain whose owner has already said it should
not, and the only copy of the removal would sit with the pusher. Clause 3 is
what admits the first push, where `pre` is empty. Once neither holds, every
later extension is refused, which is what makes an empty `WitnessSet` mean
nobody keeps this chain: the witness keeps the prefix it already stored and
stops growing it, and the chain moves on without it.

This replaces "the ledger is already stored" as an admission clause. Today
`WitnessStorage::push` never re-checks admission for a held ledger
(`crates/mabel-node/src/witness/storage.rs`, the `held` branch), so a witness
once named keeps accepting pushes for that ledger forever, which is the same as
having no removal at all.

Decision: **the tag-11 legacy clause is gated twice and off by default.** Clause
4 holds only when all three are true: `W` is non-empty; `node.json` sets
`accept_legacy_witness_config: true`; and `pre.witness_endpoints()` or
`post.witness_endpoints()` contains this node's own endpoint id. The field
defaults to false and is documented as a migration switch, to be removed with
the last pre-proposal ledger. Gating on `W` being non-empty is what keeps the
promise below: a home that witnesses for nobody cannot be pushed to through a
legacy list either, whatever the switch says.

Decision: **witnessing is config-gated and the gate names identities.**
`node.json` gains `witness_for: Vec<IdentityId>`, at most 16, empty by default,
and empty means this home witnesses for nobody: every push for a ledger it
neither holds a key for nor already stores under a live witness set is refused.
This follows decision 018's shape, where exposure is an explicit operator act
rather than a default, and it means **a wallet does not become a public dump the
moment it holds an identity**.

Decision: **`witness_for` names ids alone and needs no local key.** An entry
must parse as a 52-character identity id and the list must hold no duplicate.
It does not have to name an identity under `identities/` in this home. A witness
fleet is several machines answering for one witness identity W: one machine
holds W's keys and appends W's advertisement listing every machine, and the
others need only W's id and a copy of W's chain. Requiring `identities/W` on
every machine would mean every machine in the fleet can rewrite W, including its
principal set, which is a worse arrangement than the one this rule allows.

#### 4.1 The advertisement invariant on `witness_for`

Decision: **a home may witness for W only while W's chain advertises this home's
endpoint.** The check is: the latest non-equivocating local copy of W folds to
an `endpoints()` containing this home's `node.key` public half. It runs at
startup, again whenever this home stores a longer copy of W, and again when its
own endpoint id changes, which is what a regenerated `node.key` is.

When the check fails for an entry, that entry stops admitting **new** ledgers:
clause 3 above no longer fires for it, so a push for a ledger this home does not
store is refused. Clause 2 keeps firing, so ledgers already stored under W keep
taking extensions, and reads are untouched. Stopping mid-chain would strand
every replica this home already holds on the strength of a config file that may
simply be ahead of the advertisement it depends on. A failing entry is reported
on `GET /api/node` beside the id, and named once in the startup log with the
reason: no local copy of W, W advertises nothing, or W advertises other
endpoints and not this one. Startup does not fail; a witness whose advertisement
has not landed yet should serve what it has.

#### 4.2 The binding predicate

Decision: **an endpoint is `verified` for a witness identity under four
conditions, and the fourth is that the evidence did not come from that
endpoint.** An endpoint `E` is verified for witness identity `W` when:

1. a chain for `W` folds clean under the rules of proposal 001 section 3; and
2. that chain's seq-0 event hashes to `W`; and
3. the chain's folded `endpoints()` contains `E`; and
4. the chain was served by a source other than `E`.

Anything else is `hinted`. Condition 4 is the one a shorter rule leaves out, and
without it the label proves nothing: a machine that was named in W's
advertisement last month, and has since been dropped and compromised, can serve
the historical prefix that still names it and mark itself verified. A former
endpoint replaying a prefix stays hinted, because the only evidence for it came
from itself.

Decision: **a binding records the head seq it derives from and is never
re-derived from a shorter chain.** Bindings live in
`bindings/<identity_id>.json` under the node home, beside `peers.json` and
`verification/`:

```json
{"identity": "<W>", "endpoints": [
  {"endpoint": "<E>", "head_seq": 41, "source": "<the endpoint that served it>",
   "observed_ms": 1756000000000}]}
```

Rules on that file: an observation whose chain head seq is lower than the
recorded `head_seq` neither creates nor refreshes a binding; an observation at a
strictly greater head seq replaces the whole entry list for W, so an endpoint
absent from the newer `endpoints()` drops back to hinted; equal head seq with
divergent events is equivocation and clears every binding for W, since the
question of which chain is W's is exactly what is open. The file is a derived
cache and may be deleted: losing it costs one round of hinted labels. The
crawler still never writes a stranger's ledger under `ledgers/` (proposal 003
section 3); a binding is a derived summary, like a graph generation, not a copy.

Decision: **a pusher verifies the binding, and an unverified binding is a
warning and never a refusal.** The verification is cheap and it is a real proof.
An endpoint id is an ed25519 public key, so dialling it authenticates the remote
at the transport layer; combined with an advertisement from W's own verified
chain, the pusher has a chain of signed facts: a controller of W said E answers
for W, and QUIC proves the remote holds E's secret, therefore the remote is a
machine W named. Proposal 001 section 4 declines to use transport identity as
*authorization*, and this does not change that: the witness still authorizes
nothing on `remote_id()`. This is the pusher checking its own expectation.

The rules:

- resolution labels every candidate endpoint `verified` when 4.2's four
  conditions hold and `hinted` otherwise (a link, a ticket, DNS, `peers.json`, a
  node default, a tag-11 list);
- `sync push` dials both kinds and reports `binding` per endpoint, and the CLI
  prints, for a hinted one, that nobody's ledger confirms it;
- after a push that stored events, the wallet fetches the witness identity's own
  ledger, one `Get` whose result is checked against the requested ledger id like
  any other fetch, and **not from the endpoint it just pushed to**, so the fetch
  can establish a binding under condition 4. When no other endpoint for W is
  reachable, the fetch still runs and its result is stored, but the binding
  stays hinted and the CLI keeps printing the warning.

Refusing a hinted push is what a stricter rule would do, and it would make
bootstrap impossible: the first push to a new witness necessarily happens before
the pusher holds that witness's ledger. The harm of a hinted push is also
bounded. A ledger is public replicated data, so pushing it to a stranger leaks
nothing; what the pusher loses is a false belief that a replica exists, and the
warning is exactly the sentence that says so.

### 5. Resolution, dialling and bootstrap

Decision: **the source order of proposal 003 section 3 is replaced by eight
sources, cheapest first, then most authoritative, then most leaky.** For a
ledger `L`:

1. `Local`: a copy under `ledgers/`.
2. `CallerHint { endpoint }`: an endpoint supplied with this request, from a
   `mabel://` link, a `--peer` ticket or `--from`. A human just named it.
3. `PeerHint { endpoint }`: `peers.json` for `L`, plus what this crawl learned.
4. `NodeWitness { witness, endpoint }`: the endpoints of each identity in
   `node.json.witnesses`, resolved by 5.1. Needs no copy of anything, which is
   why it is the workhorse for a ledger this home has never seen, and why it
   stays ahead of everything the chain names, as it is today
   (`FetchSource::NodeWitness` is source 3 and `LedgerWitness` source 4 in
   `crates/mabel-node/src/graph/model.rs`).
5. `LedgerEndpoint { endpoint }`: the endpoints `L`'s own tag 18 advertisement
   names. Reachable only once another source produced a copy of `L`.
6. `WitnessIdentity { witness, endpoint }`: the endpoints of each identity in
   `L`'s `witness_identities`, each resolved by 5.1. Also needs a copy of `L`.
7. `LegacyWitnessHint { endpoint }`: the endpoints in `L`'s retired tag 11
   `WitnessConfig`. Also needs a copy of `L`.
8. `DnsEndpoint { hostname, endpoint }`: the `mabel-endpoints=` records of a
   hostname named by one of the three inputs section 6 lists.

Sources 5, 6 and 7 are the **chain-named class**, and they are three sources and
not one because their provenance differs and the difference is load-bearing.
Source 7 is a list of raw endpoints written before this proposal existed, under
a field that never promised an identity: an endpoint reached through it is never
merged into a tag-18 advertisement, never establishes a binding under 4.2, and
never reports as `verified`. It counts against the chain-named budget so that a
chain full of legacy hints cannot starve source 4.

Every applicable source is queried rather than the walk stopping at the first,
because a second answer is how equivocation is seen at all (proposal 001 section
3.7), with one exception and the budget below.

Decision: **source 8 is queried only when sources 1 to 7 produced no reachable
copy.** A DNS query tells a third-party resolver which identity this wallet is
looking for, and unlike a witness dial it is not addressed to a peer the user
chose. Proposal 003 already states that the system resolver learns every
hostname the wallet checks; this keeps that set from growing to every identity
the wallet fetches.

#### 5.1 Resolving a witness

Decision: **witness resolution is a non-recursive base operation.** Resolving
the endpoints of witness identity `X` runs the source list above with sources 4
and 6 removed: `Local`, `CallerHint` (endpoints the caller named for `X` itself,
a ticket or a `--peer`), `PeerHint`, `LedgerEndpoint` from a local copy of `X`,
`LegacyWitnessHint` from that copy, the bootstrap endpoints `node.json` records
beside `X`, and `DnsEndpoint` under the same exception. A witness's endpoints
are therefore never found by resolving that witness's own witnesses, and the
bootstrap rules of 5.3 exist precisely so this base case has raw endpoints.

Decision: **one visited-identity set per top-level operation.** Resolution
carries a `BTreeSet<IdentityId>`; an identity already in it is skipped rather
than resolved again. That terminates the two cases the rules otherwise allow: a
ledger naming itself in its own `WitnessSet` (section 1), and the same witness
appearing in `node.json.witnesses` and in the chain's witness set, which today
would resolve twice and dial the same endpoints twice.

#### 5.2 The dial budget

Decision: **one budget and one deadline per top-level operation, not per
ledger.** A top-level operation is one `sync push`, one fetch, one route call or
one crawl run. Across witness resolution, the fetches of the target ledger and
any DNS lookup, it dials **at most 16 distinct endpoints**, counted once per
endpoint id after dedupe, so an endpoint three sources name costs one slot. It
shares one deadline: the crawl's existing 60-second `RUN_BUDGET` when the
operation is a crawl, and a new 20-second `RESOLVE_BUDGET` otherwise, which is
two rounds of 8 in flight at the existing 5-second `PER_FETCH_TIMEOUT` plus
slack for a DNS lookup.

The 16 are allocated per source class, so no class can consume the budget:

| Class | Cap | Note |
|---|---|---|
| `Local` | free | no dial |
| `CallerHint` | 4 | the link caps its `endpoints` at 4 (section 7) |
| `PeerHint` | 4 | the per-ledger hint cap of 5.4 |
| `NodeWitness` | 8, with 4 reserved | 4 slots no other class may take |
| chain-named (5, 6, 7) | 8 combined | 16 witnesses at 8 endpoints is 128 candidates |
| `DnsEndpoint` | 4 | after the rest produced no reachable copy |

The reservation is the important half: without it a ledger naming 16 witnesses
spends the whole budget before the node's own configured witnesses, which are
the endpoints most likely to answer, get a single dial. The crawler's other caps
are unchanged (proposal 003 section 3: depth 2, 500 nodes, 8 fetches in flight,
5 seconds per fetch, 300 fetches, 60 seconds authoritative) and the dial budget
sits inside them.

#### 5.3 `peers.json` hygiene

Decision: **`peers.json` is a cache with a cap, an age-out and an eviction
rule.** Today it is an uncapped `BTreeMap<LedgerId, Vec<EndpointId>>` that two
paths append to and nothing ever removes from
(`crates/mabel-node/src/peers.rs`, `graph::fetcher::record_hint`,
`wallet::sync::record_hints`). Each entry becomes an object:

```json
{"endpoint": "<id>", "first_seen_ms": 0, "last_success_ms": 0, "failures": 0}
```

- at most 8 hints per ledger; over the cap, the entry with the oldest
  `last_success_ms` is evicted;
- a hint with no success in 30 days is dropped;
- three consecutive failures evict the hint, and one success resets the count;
- a bare string is read as a hint with no timestamps and no successes, so an
  existing `peers.json` loads; the file is rewritten in the new shape on the
  first write.

Decision: **a `CallerHint` endpoint is never written to `peers.json`.** An
endpoint that arrived in a link or on a command line served the operation it
came with and nothing more. Writing it back would let anyone whose link reaches
a paste into the search box install a durable dial target in this home's cache
for an identity they do not control, and it is not needed: a deliberate fetch
stores the ledger under `ledgers/`, so from the second attempt on, the
identity's own tag 18 advertisement (source 5) is the durable path, published by
its controllers rather than by whoever sent the link. Sources 4 to 8 are written
back as they are today, one hint for the source that served the kept copy, and a
source 3 hint that served it has its `last_success_ms` refreshed and its
`failures` cleared.

#### 5.4 Bootstrap

You cannot fetch witness identity W's ledger without an endpoint, and W's
endpoints live in W's ledger. Raw endpoints therefore stay first-class and none
of these paths is removed:

- an `EndpointTicket` on the command line as `--peer`, which is what the compose
  topology seeds and what `mabel node ticket` prints;
- a `mabel://` link's `endpoints` hints (section 7);
- a `mabel-endpoints=` DNS record (section 6);
- an `endpoints` entry recorded beside the witness id in `node.json`.

Decision: **every configured witness carries at least one of those four, and
`node.json` holds the durable copy.** `node.json.witnesses` becomes a list of
objects:

```json
"witnesses": [{"identity": "<W>", "endpoints": ["<E>", "<E>"]}]
```

`mabel witness set-default --witness <mabel-id> [--endpoints <endpoint,...>]`
writes both. It refuses with `unresolvable_witness` when it can neither dial an
endpoint given on the command line nor find one in a local copy of W, a claimed
hostname's `mabel-endpoints=` record or a ticket already on disk, because a
configured witness with no reachable endpoint is a config entry that does
nothing.

This reverses the tidier arrangement of keeping every endpoint hint in one
store. `peers.json` is now a cache with an eviction rule (5.3), and the one fact
that makes a configured witness reachable at all cannot live somewhere a cap can
evict it. An old `node.json` whose `witnesses` array holds 64-character hex
endpoint ids fails to load rather than being misread, because a hex endpoint id
is 64 characters and a base32 identity id is 52; the loader recognises that case
and says what to run instead.

#### 5.5 Rotating an endpoint (normative)

The advertisement replaces the whole list, so a rotation is two events and one
out-of-band update, in this order:

1. bring up the new machine and read its endpoint id with `mabel node id`.
2. a controller of W appends an `EndpointAdvertisement` naming **both** the old
   and the new endpoint. Whole replacement means the old one must be repeated or
   it is dropped in this step.
3. update every bootstrap record that names W: the `mabel-endpoints=` record in
   the zone, any published link, the `endpoints` entry in the `node.json` of
   every home that configured W, and any ticket handed out. This step is out of
   band by construction: the records are not on any ledger.
4. once readers have had a chance to fetch step 2, a second advertisement names
   the new endpoint alone. The old machine keeps serving reads until then, since
   a reader holding the previous advertisement will still dial it.

The failure state, stated plainly: a client whose only copy of W is the
advertisement from before step 2, and whose only bootstrap record still names
the old endpoint, reaches nothing once the old machine stops. It cannot learn
the new endpoint from inside Mabel, because the only copy of W's new
advertisement sits on a machine it cannot dial. Recovery is a new ticket, an
updated DNS record or a fresh link, handed over the way the first one was. This
is the permanent shape of the bootstrap problem and the reason step 3 is not
optional.

### 6. DNS endpoint hints

Decision: **one name, one query, a second key.** The record set at
`_mabel.<hostname>` gains a second recognised prefix beside `mabel=`:

```
_mabel.alice.example.  TXT  "mabel=<identity id>"
_mabel.alice.example.  TXT  "mabel-endpoints=<endpoint id>,<endpoint id>"
```

A second name, `_mabel-endpoints.<hostname>`, loses on two counts. It costs a
second query and a second leak for a fact the first query could have carried.
And `_mabel-endpoints.` is 17 bytes against `_mabel.`'s 7, so the hostname cap
for that record would be 236 where `MAX_HOSTNAME_BYTES` is 246, which means the
`hostname` field in `ProfileUpdate` would need two length rules for one
hostname, and the ledger scanner would have to enforce the tighter one.

Parsing, as strict as the existing rule and normative:

- the prefix is `mabel-endpoints=`, compared case-insensitively, exactly as
  `mabel=` is;
- within one resource record, character-strings are concatenated with no
  separator; strings are never concatenated across records (unchanged, and it is
  what `TxtRecord::value()` already does);
- the remainder splits on `,` with no whitespace anywhere; every element must
  parse under the existing case-insensitive id codec to 32 bytes and must
  decompress to a valid ed25519 point;
- **one overflow rule, discard whole, at both levels.** A record with an
  unparseable element, an empty element, a duplicate element, more than 8
  elements, or any byte outside the codec's alphabet and the comma is discarded
  whole. If the surviving records at one label name more than 8 distinct
  endpoints between them, the label's endpoint set is discarded whole and read
  as absent. Nothing is ever trimmed to fit: a partially accepted list is how a
  reader ends up dialling something the operator did not mean, and choosing
  which 8 of 9 an operator meant is a guess. Anyone who can add a record at the
  label already controls the zone, so the failure this rule allows is a
  misconfiguration and not an attack;
- surviving endpoints from several records at one label are unioned and sorted
  ascending by their rendered base32, so two wallets derive the same set from
  the same zone even though a resolver may return records in any order;
- a record beginning with neither prefix is ignored, as today;
- CNAME is followed to at most four links, unchanged (`MAX_CNAME_LINKS`).

The arithmetic behind the caps. A TXT character-string holds at most 255 bytes.
`mabel-endpoints=` is 16 bytes and an endpoint id renders as 52 characters, so
one string holds `16 + 52 + 3 * 53 = 227` bytes for 4 endpoints and would need
280 for a fifth. **Four endpoints is the most one character-string can carry.**
A zone publishing 5 to 8 splits them across two character-strings in one record,
which the concatenation rule above joins back with no separator, including
across a split in the middle of an id.

Decision: **an endpoints record is a hint about a hostname, and what it may be
read for depends on where the hostname came from.** One hostname can back
several identities (proposal 003 section 2), so the record cannot be read as a
statement about any one of them. The applicability matrix:

| The hostname came from | May yield an identity | May yield endpoints |
|---|---|---|
| the caller: a search box, `GET /api/resolve?input=`, `--from-host` | yes, from `mabel=` under the existing rules | yes, from the same response, for the identity that response resolved to |
| a ledger's own `ProfileUpdate` claim, a stale local copy of `L` or the stored crawl generation for `L` | no: the claim is what verification checks, and this is not verification | only when the same response also carries `mabel=<the identity being resolved>` |

The second row is the rule that keeps a hostname from redirecting an identity
that merely claimed it: a zone that names other endpoints but not this identity
offers this identity nothing. Source 8's inputs are exactly three, and no
hostname is ever guessed: a hostname the caller typed for this operation,
`--from-host`, and a hostname held in a stale local copy of `L` or in the stored
crawl generation for `L`. The third is the one worth the leak, because it is the
recovery path a rotation needs: a wallet holding an old copy of `L` whose every
recorded endpoint is dead can still find `L`'s new machines through the zone `L`
already claimed.

Nothing in this section affects verification. The five verification statuses
stay five and stay about `mabel=` alone: a zone with an endpoints record and no
`mabel=` record is still `unverified`, and the endpoints record is never read,
written or cached by `verification/<identity_id>.json`. A hint that reaches a
node holding nothing for you is a fetch that answers `NotFoundResp`, and nothing
is authorized by a hint (proposal 001 section 4).

### 7. The mabel link

Decision: **one compact shareable form, `mabel://<identity id>[?endpoints=...]`,
with an exact grammar and no flexibility.**

```
mabel://<identity-id>[?endpoints=<endpoint-id>[,<endpoint-id>]{0,3}]
```

- the scheme is `mabel`, followed by `://`;
- the authority is exactly one identity id under the existing id codec, 52
  characters, and nothing else: no userinfo, no port;
- the path is empty or a single `/`;
- the query holds at most the one key `endpoints`, at most once, whose value is
  1 to 4 comma-separated endpoint ids under the same codec;
- there is no fragment;
- parsing is case-insensitive for the ids, because the codec is; rendering is
  always lowercase.

Refusal rules. Any other scheme, any other query key, a repeated `endpoints`,
any path segment, a fragment, a port, userinfo, percent-encoding anywhere,
whitespace anywhere, an empty `endpoints`, a duplicate endpoint, more than four
endpoints, or an id that does not parse: refused whole, with code 2 and reason
`invalid_mabel_link`. A link with three good endpoints and one bad one is
refused, not trimmed, which is the same rule the DNS record follows and for the
same reason.

Decision: **one refusal spelling, `invalid_mabel_link`, on both surfaces, and
`ResolveStatus` gains no value.** The CLI and `GET /api/resolve?input=` both
answer code 2 with that reason and `details.input` holding the string as given.
The four `ResolveStatus` values stay four: they report what DNS said, and a
malformed input never reached DNS. The earlier draft had both spellings, which
would have made a client branch on `status` for one bad input and on `reason`
for another.

Decision: **the outer parameter is decoded exactly once, then the bytes hit a
byte-exact parser.** `GET /api/resolve?input=<value>` percent-decodes `value`
once, in the HTTP layer, which is what a query decoder does. The decoded bytes
go to the core parser unchanged, and that parser refuses percent-encoding
outright, so `%252f` decodes once to `%2f` and is refused rather than decoded
again into `/`. A repeated `input` key, or any other query key, is refused with
code 2 and the existing reason `unknown_query_parameter` (repeats carry
`details.field: "input"`). No layer below HTTP ever decodes anything.

Decision: **the link caps `endpoints` at 4 where the payload caps at 8**, on
string length alone. With `?endpoints=`, a one-endpoint link is 123 characters,
four endpoints is 282 and eight is 494. At 282 the link fits a chat message, a
printed line and a read-aloud comparison at a stretch; 494 is close to twice
that. No QR density measurement was taken, and the cap does not rest on one: a
denser square may well scan fine, and the implementing ticket may raise the cap
if someone measures it. Four endpoints is enough to reach a machine behind a
moving address, and an identity that needs more publishes them on its own chain,
where the reader gets all eight after the first fetch.

Decision: **the link is rendered lowercase and never uppercased for QR
density.** Uppercase base32 would let the encoder use QR alphanumeric mode,
which spends 5.5 bits per character where byte mode spends 8, and it is refused
anyway: every id in this system renders in lowercase base32 on every surface,
and two spellings of one id is exactly what the anti-spoofing rules of proposal
003 section 4 forbid.

Decision: **a link's hints apply to fetching the identity in that same link and
to nothing else.** Every CLI operand that takes `<alias|id>` also takes a link,
and the matrix is:

| Command and operand | The hints | Why |
|---|---|---|
| `mabel sync fetch <link>` | fetch the link's identity | the link names the ledger being fetched |
| `mabel identity show <link>`, `mabel lookup <link>` | fetch the link's identity | same |
| `mabel trust add --identity <alias\|id> --subject <link>` | fetch the subject | the subject is the ledger being fetched |
| `mabel trust add --identity <link> ...` | ignored, with a warning on stderr | `--identity` names a local signer; this home already holds it |
| `mabel witness add --identity <link> --witness <link>` | ignored on `--identity`, used to fetch `--witness` | the same split: signer local, subject fetched |
| `mabel profile replace --identity <link>` | ignored, with a warning | local signer |

Ignoring rather than refusing keeps a link usable as a way to spell an id, which
is the point of taking one everywhere. The warning names the flag so a user who
expected the hints to do something learns they did not.

Where it surfaces:

- the wallet search box takes it. `wallet-search` is relabelled `Mabel ID,
  handle or link`, and pasting a link navigates to the identity page and passes
  its `endpoints` to the fetch as `CallerHint` endpoints. Pasting a bare id
  navigates the same way with no hints, so finding the ledger falls to sources 3
  and 4, the hints this home recorded and the witnesses it uses by default.
  Before the fetch runs, the page states what using the link does: it asks the
  machines the link names for that identity, which tells those machines this
  home's network address and which identity it is looking for.
- the identity page offers `action-share`: the string with a copy control, the
  same string as a QR square, and a `.mabel` file holding one line, the link,
  UTF-8, trailing newline, no BOM. The share panel says what handing the link
  over discloses: the identity id, the machines that answer for it, and, to
  whoever uses it, this home's address.
- the CLI takes a link **anywhere an `<alias|id>` is taken**, and adds `mabel
  identity share <alias|id> [--endpoints auto|<endpoint,...>] [--out <file>]
  [--qr]`. With `--endpoints auto` the link carries the identity's own
  advertised endpoints, or this node's endpoint id when the home can sign for
  the identity and the chain advertises nothing yet.

Decision: **the parser lives in `mabel-core` and the UI never parses.** Core
already owns the id codec and is IO-free, so the grammar above is a pure
function with golden vectors, shared by the CLI, the node and the fixtures. The
browser gets no second implementation: `GET /api/resolve/:hostname` becomes `GET
/api/resolve?input=<value>` and accepts a hostname, a bare identity id or a
link, answering `{ok, input_kind, identity_id, hostname, endpoints, status}`
with `input_kind` in `identity | hostname | link`. A query parameter replaces
the path parameter because a link contains `://` and `?`. The route still writes
nothing and still touches no verification cache: navigation is not verification.

QR rendering needs one crate on each side, an encoder for the CLI's `--qr` and
an SVG encoder for the UI. Their versions are pinned by the implementing ticket
against the registry on the day, the discipline proposal 001 section 4 uses for
a dependency it has not yet compiled.

### 8. One node API and one store

Decision: **there is one router, and every node serves it.** `api::wallet` and
`api::witness` merge. `NodeRole` stops being read, `role` leaves `GET
/api/node`, and what a node can do is read from what it holds:
`identity_count` for signing, `witness_for` for accepting strangers' pushes.
`mabel serve` replaces `mabel wallet serve` and `mabel witness run`, which
survive as hidden undocumented aliases (decision 012 allows those).

Decision: **`role` in `node.json` is recognised and ignored, with one warning
line.** `NodeConfig` sets `deny_unknown_fields`, so deleting the field would
make every existing `node.json`, every seeded home and the compose entrypoint
fail to load on upgrade, for a value nothing reads. The field stays,
deserializes as before, is read by nothing, and the node logs once at startup:
the file, the key, and the fix, which is to delete the line. It is removed by
the next proposal that changes `node.json` for another reason.

Decision: **one store serves both capabilities.** `wallet/store.rs` and
`witness/storage.rs` read and write the same `NodeHome` layout already;
`WalletReadStore` is a read-only adapter with no index, no fork records, an
always-`NOT_ADMITTED` `push` and a `list` that re-folds every ledger from disk,
while `WitnessStorage` keeps an in-memory folded index, records forks and
enforces caps. The witness store becomes the one store, under the name
`node::LedgerStorage`, and the wallet adapter is deleted.

What that means for a home with an empty `witness_for`:

- the index is built at startup for its handful of ledgers, exactly as
  `WitnessStorage::open` does today, and it replaces the re-fold-per-`List` the
  wallet adapter does now. That is what makes the paging on `GET
  /api/identities/known` cheap.
- the `forks/` directory is created lazily and stays empty until this home meets
  a conflicting event on a ledger it holds, which it can: a deliberate fetch
  stores a stranger's ledger, and equivocation on it is a fact worth recording.
  A wallet gains fork records it never had, which is what makes `GET /api/forks`
  answerable on every node rather than a witness-only route.
- `push` no longer answers a flat refusal. It runs section 4's rule, and with
  `witness_for` empty and no local signing key the answer for a ledger this home
  does not store is still `NOT_ADMITTED`, with the reason naming the rule rather
  than the program: a home still stores no stranger's ledger, now because it
  witnesses for nobody rather than because its store cannot.
- `WitnessCaps` becomes `StorageCaps` and applies everywhere. The
  10000-ledger cap and `storage_capacity` from `node.json` now bound a wallet
  too, which is a bound it did not have.

`wallet/runtime.rs` and `witness/runtime.rs` merge for the same reason: they are
already the same 200 lines apart from which store the sync server answers from
and which router the HTTP server gets, and after this there is one of each.

What folds in. A witness's holdings are ledgers it stores and cannot sign for,
which is precisely what `GET /api/identities/known` already answers, and its own
witness identity is a ledger it signs for, which is `GET /api/identities`. So:

- `GET /api/ledgers`, `GET /api/ledgers/:ledger_id` and `GET
  /api/ledgers/:ledger_id/events` are removed. Their answers come from `GET
  /api/identities/known`, `GET /api/identities/:identity_id` and `GET
  /api/identities/:identity_id/ledger?since=`, which already answer for any
  ledger this home holds.
- `GET /api/identities/known` gains `offset`, `limit` and `more`, default limit
  100 and maximum 256, matching `MAX_LIST_LIMIT` in `mabel-net`. A witness holds
  up to 10000 ledgers and the route is unpaged today.
- `GET /api/forks` keeps its name and its optional `ledger_id` query on every
  node. A fork is a fact about a stored ledger and no other route reports it.
- `GET /api/witnesses` rows become identities: `{identity_id, display_name,
  endpoints: [{endpoint_id, binding}], named_by, is_node_default, stored}`.
- `GET /api/witnesses/:endpoint_id/ledgers` becomes `GET
  /api/witnesses/:identity_id/holdings`, resolving the identity to endpoints
  through section 5 before proxying `List`. The last segment changes because the
  shape does not: both keys render as 52 base32 characters, so a client still
  sending an endpoint id to `/ledgers` would parse, dial nothing and get a
  confusing 502. A new segment gives it a 404 instead. `witness_unreachable`
  keeps its spelling and its 502, and `details` names the identity and every
  endpoint tried.
- `POST /api/identities/:identity_id/endpoints` is new: it appends one
  `EndpointAdvertisement`, body `{endpoints: [...]}`, whole replacement, and
  refuses a no-op the way the profile route does.
- `POST /api/identities/:identity_id/witnesses` keeps its path and its body
  names identity ids.
- `POST /api/identities/:identity_id/fetch` keeps `from`, an endpoint id, now
  meaning "dial this endpoint first as a `CallerHint`" with no witness lookup
  behind it, so its `unknown_witness` refusal is deleted: a bare endpoint is a
  first-class hint under section 5 and refusing one would break bootstrap. A new
  optional `from_witness` names a witness identity id and resolves it through
  5.1. At most one of the two, or code 2 and reason `conflicting_source`. The
  two keys stay separate rather than one polymorphic key because both values are
  52 base32 characters and nothing in the string says which it is.

Decision: **an id at a value surface must resolve to an identity, and an
endpoint id is refused outright.** `mabel witness add --witness <id>` and the
`witnesses` array of `POST /api/identities/:identity_id/witnesses` refuse:

- an id this home cannot resolve to a known identity, meaning it neither holds a
  copy nor fetches one within the section 5.2 budget during the call: code 2,
  reason `unresolvable_witness`, `details.witness` naming it and
  `details.endpoints_tried` naming what was dialled. The bootstrap for this is
  `--endpoints`, which 5.4 already requires for a configured witness;
- an id equal to an endpoint id this home knows, meaning its own endpoint id or
  any endpoint in a stored advertisement, a stored tag 11 list or `peers.json`:
  code 2, reason `endpoint_not_identity`. This one is refused before any dial,
  because the id is not ambiguous, it is wrong.

`api/parse.rs::witnesses` keeps `witnesses_out_of_range` and
`duplicate_witness`, its message becomes `witnesses must hold 0 to 16 distinct
identity ids`, and `malformed_endpoint_id` becomes `malformed_identity_id`.

What still gates. Nothing gates a route. A home with no identities answers
`{"ok": true, "identities": []}`, which is emptiness and not a refusal. The
mutating routes exist everywhere, and a mutating route naming a ledger this home
holds but cannot append to answers 403, code 2, reason `no_local_signer`, with
the message `this home holds no key that may append to <id>`. That is a new
case in `contracts/cli/errors.json`; a witness today answers 404 `no route for
POST /api/trust`, which stops being true the moment there is one router.
`unknown_ledger` keeps its meaning for a ledger the home does not hold, and the
witness-only spelling `ledger_not_held` dies with the witness routes.

Static segments keep the rule the `known` route already relies on: an identity
id is 52 base32 characters, so no id can collide with a short static segment,
and the router matches the static segment first. `known`, `holdings`,
`endpoints`, `witnesses`, `fetch`, `ledger` and `keys` are all covered by that
one sentence, and it is the reason the drill-in route can change its last
segment without ambiguity.

Decision: **`List` answers only the ledgers a home is willing to be known to
hold.** Today `List` enumerates everything stored, with no filter and no gate
(`crates/mabel-net/src/server.rs`), which was harmless while the only nodes that
published an address were witnesses. Once any identity can advertise its
endpoints, a wallet that advertises makes its whole stored set enumerable by
anyone who dials it, and a wallet's stored set is the list of identities its
owner has fetched or been pushed. So `List` narrows to: the ledgers this home
signs for, plus, when `witness_for` is non-empty, the ledgers it admitted as a
witness. A ledger it merely fetched is served by `Get` to anyone who can already
name its id, and is never enumerated. The narrowing keeps
`/api/witnesses/:identity_id/holdings` answering what it answers today, since a
witness's holdings are exactly the admitted set.

What the UI shows. Nav is three entries on every node, `nav-wallet`,
`nav-witnesses` and `nav-node`; `nav-witness` and the whole `/witness` route
tree are removed, along with the `witness-detail-*` testids and
`WitnessCard.tsx`. A witness's home page is the wallet home page: its own
identity under "Your identities", its holdings under "Known identities". The
`/witnesses` list draws identity cards, and the facts the witness card carried
move onto the witness identity's own page as rows and a "What this witness
holds" section. `/node` loses `node-role` and gains `node-witness-for`, the
identities this node witnesses for, reading `none` when it witnesses for
nobody. The identity page gains a `machines` row, the advertised endpoints, and
two actions, `action-endpoints` and `action-share`.

A home with no keys offers no mutating action, and the UI decides that per
identity from data it already has, not from a route or a mode: an identity card
draws its actions when the identity appears in `GET /api/identities`, the
signing set, and draws none when it appears only in `GET
/api/identities/known`. A home whose `GET /api/identities` is empty therefore
shows a wallet with rows and no buttons, and the `/node` page states in one
sentence that this home holds no keys and names what it does hold.

Two wording rules for the binding labels, from decision 017. First, `binding`,
`verified` and `hinted` are API and CLI words and never appear in UI copy: the
`machines` row says "listed on this identity's own record" or "not confirmed by
any record we have", in a full sentence on its own line. Second, each machine is
one row with labelled values, never an id and a status joined by a middle dot or
a dash, and the id is rendered as the same lowercase base32 the rest of the UI
uses with no separators inserted for readability.

Decision: **publishing an advertisement asks for consent once per home**, the
panel `handle-consent` already establishes for a hostname. The panel states
three things: the endpoint id stays readable forever by anyone who can name the
ledger id; anyone who reads it can dial that machine directly, which reveals the
machine's address to them and to the relay; and once this home answers at a
published address, anyone who dials it can list the identities it signs for and,
if it witnesses, the ledgers it keeps as a witness.

### 9. Contracts and fixtures

Every affected fixture, classified. "Shared" means one document now describes
both capabilities.

| Fixture | Class | Note |
|---|---|---|
| `http/witness-get-node.json` | removed | `GET /api/node` is one route with one document |
| `http/witness-get-ledgers.json` | removed | `GET /api/identities/known` answers it |
| `http/witness-get-ledger.json` | removed | `GET /api/identities/:identity_id` answers it |
| `http/witness-get-ledger-events.json` | removed | `.../ledger?since=` answers it |
| `cli/witness-run.json` | removed | `mabel serve` is one command |
| `http/witness-get-forks.json` | renamed | to `http/node-get-forks.json` |
| `http/wallet-get-node.json` | renamed, shared | to `http/node-get-node.json`: `role` out, `witness_for` in |
| `http/wallet-get-witness-ledgers.json` | renamed | to `http/wallet-get-witness-holdings.json`, new route segment |
| `cli/wallet-serve.json` | renamed | to `cli/serve.json`, shutdown document gains `witness_for` |
| `http/wallet-get-identities.json` | changed | `witnesses` resolved, `endpoints` and `witness_endpoints` added |
| `http/wallet-get-identity.json` | changed | same |
| `http/wallet-post-identities.json` | changed | same |
| `http/wallet-get-known-identities.json` | changed | paging |
| `http/wallet-get-identity-ledger.json` | changed | two new `payload_kind` values |
| `http/wallet-post-identity-witnesses.json` | changed | identity ids, new refusal reasons |
| `http/wallet-get-witnesses.json` | changed | rows are identities with `binding` |
| `http/wallet-get-resolve.json` | changed | `?input=`, `input_kind`, `endpoints` |
| `http/wallet-post-sync-push.json` | changed | per-witness rows carry the witness identity, the endpoint and `binding` |
| `http/wallet-post-identity-fetch.json` | changed | `from_witness`, `unknown_witness` gone |
| `http/wallet-post-identity-endpoints.json` | new | `POST /api/identities/:identity_id/endpoints` |
| `cli/identity-share.json` | new | `mabel identity share` |
| `cli/identity-endpoints-replace.json` | new | `mabel identity endpoints replace` |
| `cli/witness-add.json` | changed | `--witness <alias\|id>`, `--endpoints` |
| `cli/witness-set-default.json` | changed | identity plus endpoints in `node.json` |
| `cli/sync-push.json` | changed | `binding` per endpoint |
| `cli/sync-fetch.json` | changed | `--from-witness`, `--from-host` |
| `cli/identity-create.json` | changed | witnesses render as identities |
| `cli/identity-list.json` | changed | same |
| `cli/identity-show.json` | changed | same, plus the machines row |
| `cli/dev-seed.json` | changed | the seed creates a witness identity |
| `cli/errors.json` | changed | `no_local_signer`, `invalid_mabel_link`, `unresolvable_witness`, `endpoint_not_identity`, `conflicting_source` |
| the other 15 `http/*.json` | unchanged | graph, lookup, contact, keys, profile, verification, the four membership routes, the two trust routes |
| the other 17 `cli/*.json` | unchanged | contact, graph, lookup, the five membership commands, trust, verify, `node id`, `node ticket`, export, profile |

The `wallet-*` prefix stays on the fixtures that keep it. Those routes are about
the identities a home signs for and what it does with them, which is a wallet
whatever else the node does; witnessing adds no route, so there is no
`witness-*` half left to be symmetrical with, and renaming 25 files buys a diff.

`contracts/README.md` statements that become false and are rewritten in the same
change:

- the index rows for the five removed fixtures and the four renamed ones,
  including the route line for
  `/api/witnesses/:endpoint_id/ledgers?offset&limit`;
- "`wallet serve` and `witness run` print their document when the process stops,
  so their one case is the shutdown document": one command, one case;
- the frozen payload table, at eight rows: it gains `witness_set` (`witnesses`)
  and `endpoint_advertisement` (`endpoints`), taking it to ten, and the
  `witness_config` row is marked readable but never written. This is the third
  amendment to that freeze, after tag 17 and after `email`;
- "`GET /api/identities` and `GET /api/ledgers` sort by ascending id": the
  second route is gone, and `List` narrows (section 8), which the ordering
  bullet should say;
- the `GET /api/witnesses/:endpoint_id/ledgers` row-shape bullet: the path and
  the key type both change, the six row keys do not;
- "A witness that cannot be dialled ... naming the endpoint in
  `details.endpoint_id`. One spelling covers the ledger list and the fetch
  route": `details` now names the witness identity and every endpoint tried, and
  the fetch route no longer refuses an unknown endpoint at all;
- "`GET /api/resolve/:hostname` runs one TXT lookup ... Its four statuses": the
  path becomes `?input=`, the input widens to three kinds, the statuses stay
  four, and a malformed input is `invalid_mabel_link` rather than a fifth
  status;
- "`POST /api/identities/:identity_id/fetch` ... A `from` naming an endpoint
  this wallet knows no witness at is refused with code 2 and reason
  `unknown_witness`": deleted;
- "The witness routes keep `ledger_not_held`": the witness routes are gone and
  `unknown_ledger` is the one spelling.

### 10. Impact inventory

- **`proto/mabel/v0/ledger.proto`**: `WitnessSet` and `EndpointAdvertisement`,
  `EventBody.payload` variants 18 and 19, a comment on `WitnessConfig` saying it
  is readable and never written, and the header note that tags 10 to 19 are now
  spent. `files.proto` is unchanged; `sync.proto` gains a comment on `ListReq`
  recording that a node lists only what section 8 allows.
- **`mabel-core`**: `validate.rs` gains `WITNESS_SET` and
  `ENDPOINT_ADVERTISEMENT` descriptors, `MAX_ENDPOINTS`, and two `EVENT_BODY`
  variants; `fold.rs` replaces `witnesses` with the three accessors of section
  3; `sign.rs` gains `build_witness_set` and `build_endpoint_advertisement` and
  puts `build_witness_config` behind a test-only gate; a new module owns the
  `mabel://` grammar. Vectors: new golden vectors for a witness set, an empty
  witness set, an advertisement and an empty advertisement, plus link-parsing
  vectors; new rejection vectors for 17 witnesses, 9 endpoints, a duplicate in
  either list, a wrong-length entry, and an endpoint that is not a valid point.
  Every existing vector keeps its bytes: no existing rule changed.
- **`mabel-net`**: `LedgerSummary` and every frame are untouched and
  `RejectCode::NotAdmitted` keeps its meaning; a pusher never tells a witness
  which witness identity it means, because the witness reads that from the
  chain. `server.rs` takes the `List` narrowing of section 8 as a `Store`
  contract: `Store::list` answers the enumerable set, not the stored set.
- **`mabel-node`**: `witness/storage.rs` becomes the one store as
  `node::LedgerStorage`, taking the four-clause admission rule, the
  `witness_for` set and the caps rename, and `wallet/store.rs` is deleted;
  `wallet/runtime.rs` and `witness/runtime.rs` merge into one runtime;
  `config.rs` retypes `witnesses` to `{identity, endpoints}` objects, adds
  `witness_for` and `accept_legacy_witness_config`, and keeps `role` recognised
  and ignored; `peers.rs` takes the hint objects, the cap, the age-out and the
  eviction rule; a new `bindings.rs` owns `bindings/<identity_id>.json`;
  `graph/model.rs` and `graph/fetcher.rs` take the eight `FetchSource` variants,
  `plan_sources`, the visited-identity set and the dial budget; `graph/crawl.rs`
  takes the shared deadline; `wallet/core.rs::witnesses_of` and `wallet/sync.rs`
  take resolution, the binding label and the witness-ledger fetch from a
  different endpoint; `verification/verify.rs` gains the `mabel-endpoints=`
  prefix, its parser and the applicability matrix; `api/wallet.rs` and
  `api/witness.rs` merge into one router with the routes of section 8;
  `api/parse.rs::witnesses` accepts identity ids and takes the two new refusals;
  `api/documents.rs` takes the document changes, and `WitnessNode.witnesses`
  stops being hardcoded empty, because a node that witnesses may also push.
  `home.rs` gains the `bindings/` path; `keys.rs` is unchanged.
- **`mabel-cli`**: `witness add --identity <alias|id> --witness <alias|id>
  [--endpoints <endpoint,...>]`; `witness set-default --witness <mabel-id>
  [--endpoints ...]`; `identity endpoints replace --identity <alias|id>
  --endpoints auto|<endpoint,...>`; `identity share`; `serve` with `wallet
  serve` and `witness run` hidden; `sync fetch --from-witness` and
  `--from-host`; every `<alias|id>` argument also takes a link under section 7's
  matrix; `node ticket` and `node id` unchanged, because they are the bootstrap.
  `dev seed` creates a witness identity, advertises the seeding node's endpoint
  on it, and names it in each seeded ledger's `WitnessSet`.
- **UI**: `components/WitnessCard.tsx`, `routes/witness/` and
  `routes/witnesses/WitnessLedgersPage.tsx` deleted, with the notes of
  `routes/witness/notes.ts` moving to the "Known identities" section;
  `WitnessConfigPanel.tsx` names identities and keeps its read-modify-write,
  since the set is still replaced whole; `/witnesses` and `/node` draw identity
  cards; `nav-witness` and the role branch in `App.tsx` deleted; the `machines`
  row with the two sentences of section 8, `action-endpoints` and `action-share`
  with the QR added; `wallet-search` relabelled; `api/types.ts` retypes
  `WitnessSummary` and `SetWitnessesRequest`; the mock store and the UI tests
  follow the fixtures.
- **e2e and stories**: story 005 is rewritten to the unified surface and loses
  every `witness-detail-*` assertion; story 001 exchanges a link where it
  exchanges a descriptor; story 004's two witnesses become two witness
  identities; story 007 gains a `mabel-endpoints=` record and a resolve-by-link
  case. Two new stories: reaching an identity by link with no witness in the
  topology at all, and rotating a witness's endpoint through section 5.5 with a
  client that only holds the stale advertisement.
- **compose and images**: `docker/entrypoint.sh` grows one step, creating the
  witness identity, advertising the container's endpoint on it and listing it in
  `witness_for`; `MABEL_PUBLISH_TICKET` writes a third file beside
  `<prefix>.ticket` and `<prefix>.id`, the witness's Mabel id, and
  `MABEL_WITNESSES` holds Mabel ids with their endpoints where it held endpoint
  ids. The ticket stays and stays load-bearing, because it is the bootstrap, and
  the two-phase bring-up of `tests/e2e/lib/docker.ts` stays for the same reason:
  a witness's identity exists no earlier than its endpoint did. `witnessId()`
  reads the new file. `compose.yaml`, `compose.two-witnesses.yaml`,
  `compose.dns.yaml`, `docker/smoke.sh` and the zone files under `docker/dns/`
  all change.

### 11. Ticket cut

| Ticket | Scope | Depends on |
|---|---|---|
| 033 payloads and the write path | tags 18 and 19, descriptors, field table, fold accessors, `build_witness_set` and `build_endpoint_advertisement`, `build_witness_config` behind a test-only gate, `witness add` and `POST .../witnesses` writing tag 19, the endpoints route, `dev seed`, golden and rejection vectors, and the minimum admission and config change that keeps push working: `witness_for` in `node.json` and the tag-19 clause | nothing |
| 034 admission, `witness_for` and bindings | the four-clause rule with the pre-push state, the gated legacy clause, the advertisement invariant and its recheck, `bindings/<id>.json` and the 4.2 predicate, the witness-ledger fetch from another endpoint | 033 |
| 035 resolution and hints | the eight sources, the visited-identity set, the dial budget and shared deadline, witness resolution as a base operation, `peers.json` objects with cap, age-out and eviction, the `node.json` bootstrap endpoints | 034 |
| 036 links and DNS | the `mabel://` grammar in core with vectors, the outer decode rule, `mabel-endpoints=` parsing with the overflow rule and the applicability matrix, `GET /api/resolve?input=`, `mabel identity share`, QR and file output | 033 |
| 037 one router, one store | `api::wallet` and `api::witness` merged, the two runtimes merged, `WalletReadStore` deleted, `LedgerStorage` serving both, `/api/ledgers*` folded in, paging on `known`, the `holdings` segment, the `List` narrowing, `mabel serve`, `role` recognised and ignored | 035, 036 |
| 038 fixtures and contracts | every fixture in the section 9 table, the `contracts/README.md` rewrites, the payload table amendment | 037 |
| 039 UI | identity cards for witnesses, `/witness` removed, the `machines` row and its two sentences, share and endpoints actions, the consent panel text, the mock store and UI tests | 038 |
| 040 topology and stories | entrypoint seeding, `MABEL_WITNESSES` and the third published file, compose overlays, zone files, story rewrites and the two new stories | 039 |

The sequence keeps a working push path at every step. 033 flips the write path
to tag 19, which is why it carries `witness_for` and the tag-19 admission clause
in the same ticket: without them the first ledger written with a `WitnessSet`
could not be pushed anywhere. 034 tightens that rule without narrowing it for
any chain 033 produced, since clause 3 still admits a first push. 035 changes
which endpoints get dialled and not whether a push is admitted. 037 moves the
store but keeps the rule 034 installed, and its acceptance includes a push and
a fetch against a two-node topology before the fixtures are touched. 038 to 040
change documents, screens and topology and no admission or resolution rule.

## Alternatives considered

- **Redefining tag 11 in place**, taking the proposal 002 exception a second
  time. It lost on the exception being spent by its own words and on the failure
  mode: an endpoint id and an identity id are both 32 opaque bytes, so this is
  the one redefinition no decoder can catch, and every chain written before the
  change would fold cleanly into a wrong answer.
- **Retiring tag 11 outright**, rejecting it as a payload. Loud, which is better
  than silent, but it invalidates existing chains entirely where keeping it
  readable costs one folded field and turns the old list into a working dial
  hint.
- **Keeping "the ledger is already stored" as an admission clause.** It is what
  the code does today and it means a witness once named can never be removed:
  the stored copy admits every later extension whatever the witness set says.
- **The tag-11 legacy clause ungated.** With `witness_for` empty and the clause
  live, any chain carrying a pre-proposal `WitnessConfig` that happens to name
  this endpoint could push to a plain wallet, which is exactly the dump the
  config gate exists to prevent.
- **Merging tag-11 endpoints into the tag-18 advertisement** at the fold or in
  one `FetchSource` variant. Rejected: a tag-11 entry is an endpoint someone
  else's chain guessed at, and a tag-18 entry is a fact the identity's own
  controller signed. Merging them means the `verified` label stops meaning
  anything, and the provenance a warning has to name is gone.
- **Requiring every `witness_for` entry to name an identity under
  `identities/`.** It reads like a safety check and it forces the opposite:
  every machine in a witness fleet would hold the witness identity's signing
  keys and could rewrite its principal set.
- **One combined payload at tag 18**, witnesses and endpoints in one
  whole-replacement message. Rejected: the two facts change at different rates
  and mean different things. The witness set is a policy statement gating
  admission; the endpoint list is a transport fact that churns when a machine
  moves. Coupling them means every machine replacement re-signs an
  admission-relevant policy.
- **Naming witnesses by endpoint id and binding on the witness's ledger**, an
  event saying "this endpoint is mine". Same facts, inverted: every reader would
  need a reverse index from endpoint id to identity, built by crawling, to
  answer "who is this witness".
- **Endpoints in `ProfileUpdate`**. Rejected: whole replacement over a shared
  document means changing a machine clears a display name unless the client
  resends it, which is the exact failure proposal 003 designed the required-keys
  body to prevent, and reachability is not a public identity fact of the same
  kind as a name.
- **Keeping every endpoint hint in `peers.json` alone**, with nothing in
  `node.json`. It was the tidier answer until `peers.json` got an eviction rule:
  a cache that ages hints out cannot hold the one fact that makes a configured
  witness reachable.
- **Writing a link's endpoints back to `peers.json`** once one served a matching
  chain. Rejected: a paste into a search box would install a durable dial target
  for an identity the pasting user does not control, and the identity's own
  advertisement replaces the hint after the first fetch anyway.
- **A second DNS name, `_mabel-endpoints.<hostname>`**. Two queries, two leaks,
  and two hostname length caps for one hostname, 236 against 246, one of which
  the ledger scanner would have to enforce.
- **Trimming an over-long DNS record or link to the cap** instead of discarding
  it. It makes the reader dial an endpoint the operator did not choose, and
  picking which 8 of 9 the operator meant is a guess dressed as a rule.
- **A fifth `ResolveStatus` value, `invalid`.** Two spellings for one refusal: a
  client would branch on `status` for a malformed link and on `details.reason`
  for a malformed hostname.
- **An `https://` universal link instead of a scheme**, `https://mabel.example/
  <id>?endpoints=`. Needs a domain and a server that resolves it, which decision
  006 forbids, and it puts one operator between every two people who exchange an
  identity.
- **Refusing a push whose binding is unverified.** It makes the first push to
  any new witness impossible, since the pusher cannot hold the witness's ledger
  before reaching it.
- **Upgrading a binding on evidence the endpoint itself served.** Cheaper by one
  dial and worth nothing: a dropped endpoint can serve the historical prefix
  that still names it and mark itself verified.
- **Append semantics for the advertisement.** It needs a removal payload, and
  until one exists a reachability list can only get more wrong.
- **Keeping two API surfaces** and adding the witness routes to the wallet
  router. Rejected: it keeps two documents for one ledger and two names for one
  screen, which is the confusion the merge removes.
- **Keeping two stores**, the read-only wallet adapter beside the witness
  storage. Rejected: they read the same directories, and the wallet half would
  have to grow an index and fork records anyway to answer paged `known` and the
  forks route on every node.
- **Deleting `role` from `node.json`.** `deny_unknown_fields` means every
  existing home, seed and entrypoint would fail to load on upgrade for a value
  nothing reads.
- **Leaving `List` enumerating everything stored.** It turns an advertisement
  into a published index of every identity a wallet's owner has looked up.
- **Signed DNS proofs for the endpoints record.** It would make a dial hint look
  like a checked fact, and decision 015 says advisory.

## Consequences

Easier: a witness can move. Replacing a machine is one event on the witness's
own ledger, and every ledger that names the witness keeps working with nothing
appended. A witness has a name, a hostname and a profile, so the one thing the
wallet dials most is drawn by the same identity card as everything else, and
`/witnesses` stops being a list of numbers. An identity can be reached with no
witness at all, because a wallet already serves reads and now has somewhere to
publish where it listens. A witness can also be removed: an empty `WitnessSet`
stops later extensions from being admitted, which is a thing tag 11 could not
say.

Harder: two reachability payloads exist for one ledger for the rest of the POC,
tag 11 readable and tag 19 written, and the admission rule carries a legacy
clause behind a switch that has to be tested both ways. Resolution grows from
four sources to eight, with a dial budget, a per-class allocation, a
visited-identity set and a DNS exception that all need their own tests. Every
node now runs one store with an index and a fork directory, so a home that holds
three ledgers pays a startup fold it did not pay before. The bootstrap problem
is real and permanent: raw endpoints in tickets, links, DNS and `node.json` are
load-bearing, so "witnesses are identities" is true of the steady state and
never of the first contact. Section 5.5 writes down the rotation that follows
from that, including the case a client cannot recover from without an
out-of-band record. And tags 10 to 19 are now spent, so the next payload comes
out of the block reserved for the proposal 002 section 9 deferrals.

The binding label has a residual gap, stated rather than closed. Condition 4 of
4.2 proves the evidence did not come from the endpoint it vouches for, and
nothing more. Two endpoints that answer for the same witness can vouch for each
other, and a source serving a valid but truncated prefix at or above the
recorded head seq keeps a dropped endpoint verified until this home sees a
longer chain. Recording the head seq bounds how far back that can reach and
stops a shorter chain from re-establishing a binding, but a home that never
fetches a longer copy never learns. `verified` therefore means "some other party
served a chain for W naming this endpoint at seq N or later", which is what the
CLI and the API should be read as saying.

Advertising has a second cost beside the address. Once a home answers at a
published endpoint, anyone who dials it can page through what `List` allows: the
identities it signs for, and, if it witnesses, the ledgers it keeps as a
witness. That set is narrowed from everything stored (section 8) so that
identities a person merely looked up are not enumerable, but the signing set is,
and the consent panel says so before the first advertisement is appended.

Privacy, stated plainly and shown before the fact: an `EndpointAdvertisement`
publishes the public key of a machine, permanently, to anyone who can name the
ledger id, and anyone who reads it can dial that machine, which reveals the
machine's address to them and to the relay. A `mabel-endpoints=` record does the
same in public DNS. The resolver leak also widens: proposal 003 gave the system
resolver the hostnames a user typed and the hostnames a verification refresh
checked, and source 8's third input adds the hostname held in a stale local copy
of any ledger whose recorded endpoints have all gone dead, which is a set the
user never types. Pasting a link discloses in the other direction: the machines
the link names learn this home's network address and which identity it is asking
for. None of it is revocable in the sense a user means by the word: a later
event changes what the fold reports, and the bytes stay in every replica forever
(decision 003).

Deferred: nothing about payment or incentives for running a witness; no
multi-hop relaying, so no node forwards a push on another's behalf and Iroh's
relays stay pure transport (decision 006); key rotation still out of scope
(decision 008), including the sharp edge that a regenerated `node.key` is an
endpoint rotation and needs a new advertisement, which is a different thing from
rotating an identity key and is the only one of the two this proposal makes
possible.
