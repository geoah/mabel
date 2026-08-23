# 001: mabel architecture

- Date: 2026-08-23
- Status: accepted (2026-08-23, after dual review by Codex and an independent
  Opus reviewer; 21 arbitrated revision items applied)
- Superseded in part by [002-unified-ledger.md](002-unified-ledger.md)
  (2026-08-24): one ledger type with a principal set replaces the person and
  organization ledger kinds. Affected here: 3.3 kind-as-discriminator, 3.4
  payload table and Org* messages, 3.6 seq-0 seeding, 9 org commands, 10
  /orgs routes. Proposal 002 section 10 lists every delta.
- Decisions affected: implements 001, 003, 005, 006, 007, 008; amends 002,
  interprets 004

## Context

Mabel must reproduce hearsay's claim (a person or an organization can make a
signed, independently verifiable statement that it personally knows someone) on
a minimal hash-chained ledger plus Iroh, without KERI. The decision records fix
the product shape and leave open the event encoding, the wire protocol, the
crate cut, storage and the UI stack, which this proposal settles precisely
enough to cut into tickets. Inputs: `docs/decisions/` (authoritative, cited as
`decisions/NNN-name`), `docs/research/001-hearsay-digest.md` (flags W, A, P, L,
R, D and pitfalls 1 to 8, cited by number),
`docs/research/002-iroh-research.md`. Judgment calls are marked "Decision:" so
reviewers can target them.

## Proposal

### 1. Goals and non-goals

Goals (decisions/001-scope): one Rust core library powering every node type with
no networking in it; one append-only, hash-chained, ed25519-signed ledger per
identity, verifiable from nothing; person and org identities, one-way trust
attestations and revocations, org membership by invitation plus signed
acceptance; replication over Iroh to passive witnesses; a `mabel` CLI, a wallet
UI, a witness debug UI, container images, and end-to-end tests driving all
three. Non-goals (decisions/008-out-of-scope): key rotation, witness receipts
and thresholds, challenge-response proof of control, multi-approval org
governance, encrypted key storage, backups, wasm and mobile builds.

Ledgers are public, replicated data: every event, including every trust
attestation and revocation, is visible to anyone who can name the ledger id, and
partial disclosure is a non-goal, since mabel cannot prove one attestation
without handing over the ledger prefix containing it. Private keys never leave
the node home; everything else is publishable.

Verified means "this identity signed this statement at this position in its
chain". It is not proof that the statement is true, not proof of legal identity,
and not proof of unique humanity. That sentence goes in verifier output, the
README and this proposal (pitfall 8).

### 2. System overview

Two node roles, one binary. A **wallet node** holds private keys for many
identities (persons and orgs), appends events to the ledgers it controls, pushes
them to witnesses, fetches other ledgers to verify claims, and serves an HTTP
API plus the UI on loopback; it also serves the Iroh protocol read-only so peers
can fetch its ledgers. A **witness node** is a passive replica
(decisions/005-witnesses): it holds full copies of ledgers it does not control,
verifies before storing, serves reads, records forks, and exposes a debug UI
listing what it holds, signing nothing and holding no identity keys.

Wallets, witnesses and verifiers talk over Iroh (section 5); browsers talk HTTP
JSON to the node on loopback; humans use the `mabel` CLI against the node home
on disk. There is no application server and no database
(decisions/006-networking).

### 3. Ledger specification

#### 3.1 Event encoding, canonical form and byte authority

Decision: **protobuf (proto3) encoded with prost**, per decisions/007, chosen
over postcard and canonical JSON on the requirement neither meets: the owner
plans iOS, Android and web clients, and a checked-in `.proto` is the schema they
build from.

**The encoded bytes are authoritative.** A signer serializes once, and those
exact bytes are hashed, signed, stored and shipped; a verifier hashes and checks
the bytes it received and decodes only to read fields, never re-serializing. So
any re-encoding invalidates the signature, and digest stability is a storage
discipline: nothing may decode-then-encode an event, and only the signing path
produces event bytes (pitfall 1).

Decision: because protobuf allows several encodings of one message, mabel
defines a **normative canonical encoding** in prose next to the `.proto` that
every signed or hashed message must use: fields in ascending field-number order;
minimal varints; no proto3 default value serialized; each non-repeated field
exactly once and every field the table in 3.4 marks required present; and no
packed repeated fields in signed messages, all of which are `bytes` or message
elements and so length-delimited per entry.

Decision: the strict-parsing gate (pitfall 2) is a **wire-format validator in
`mabel-core` that scans the received bytes directly**, before and independently
of prost decoding, rejecting unknown field numbers, duplicate non-repeated
fields, out-of-order fields, non-minimal varints, wrong wire types, unrecognised
`oneof` variants and `*_UNSPECIFIED` enum values. Scanning bytes closes the gap
prost's silent unknown-field drop leaves and is deterministic across
implementations; `encoded_len() == len` may remain a debug assertion but is not
the gate. A second implementation must emit this canonical form, and the
`.proto`, this prose and the golden vectors are the cross-language contract.

Decision: BLAKE3-256 everywhere, matching hearsay, with domain separation:

```
event_id       = BLAKE3(b"mabel/event/v0\n"   || event_body_bytes)
sign_input     =        b"mabel/sig/v0\n"     || event_body_bytes
accept_input   =        b"mabel/accept/v0\n"  || acceptance_bytes
reserve_commit = BLAKE3(b"mabel/reserve/v0\n" || reserve_public_key)
```

Ids display as lowercase RFC 4648 base32 without padding (52 characters), Iroh's
`EndpointId` alphabet, with no type prefix. `proto/mabel/v0/ledger.proto` is
normative, field numbers and `oneof` tags are append-only, and a breaking change
means a `v1` directory, a new envelope `version` and a new ALPN.

#### 3.2 Event envelope

```protobuf
message EventBody {
  uint32 version      = 1;  // 0, so absent under the canonical encoding
  bytes  ledger       = 2;  // 32 bytes; absent at seq 0
  uint64 seq          = 3;  // 0-based, strictly +1 per event
  bytes  prev         = 4;  // previous event_id; absent at seq 0
  uint64 timestamp_ms = 5;  // unix milliseconds, ledger order
  bytes  author_key   = 6;  // 32-byte ed25519 public key
  oneof payload {           // tags 10 to 17, append-only
    PersonInception person_inception = 10; OrgInception org_inception = 11;
    WitnessConfig witness_config = 12; TrustAttestation trust_attestation = 13;
    TrustRevocation trust_revocation = 14; OrgInvite org_invite = 15;
    OrgAcceptance org_acceptance = 16; OrgRemoval org_removal = 17;
  }
}
message SignedEvent {
  bytes body = 1;  // the exact encoded EventBody bytes
  bytes sig  = 2;  // 64-byte ed25519 signature over sign_input
}
```

Decision: two messages, not the single `Event` message decisions/007 sketches: a
signature cannot sit inside the message it signs without a placeholder rule, and
an opaque `bytes` body makes byte authority mechanical. Decision: no separate
key id space, so `author_key` is the public key itself and key resolution is a
set membership test.

Decision: **timestamps express ledger order, not reliable wall time.** An
appender sets `timestamp_ms = max(now_ms, prev.timestamp_ms)`, so a lagging
clock cannot produce an unappendable ledger. Verification requires
non-decreasing values and a constant upper bound, `timestamp_ms <=
4102444800000` (year 2100), so one poisoned value cannot brick future appends.
Nothing is checked against the verifier's clock, because verification must be
deterministic from the bytes alone.

#### 3.3 Identity id derivation

`identity_id = ledger_id = event_id of the seq-0 event`, so naming an identity
commits to its exact inception bytes, including the active key and the reserve
commitment, and an inception from an untrusted source is checked by recomputing
the digest. Both inceptions carry an explicit `kind` field (`PERSON` or `ORG`);
the `oneof` tag already separates them, but the explicit field is what the
no-collision rule rests on and it survives re-encoding (pitfall 6; hearsay
worked around this by permuting witness order). Inceptions also carry a 16-byte
`nonce`, since two orgs founded by the same controller in the same millisecond
would otherwise share an id; later events need none.

#### 3.4 Payload types

```protobuf
enum IdentityKind { IDENTITY_KIND_UNSPECIFIED = 0; PERSON = 1; ORG = 2; }
enum Role         { ROLE_UNSPECIFIED = 0; MEMBER = 1; CONTROLLER = 2; }

message PersonInception  { IdentityKind kind = 1; bytes active_key = 2;
                           bytes reserve_commit = 3; bytes nonce = 4; }
message OrgInception     { IdentityKind kind = 1; bytes founder = 2;
                           bytes founder_key = 3; bytes founder_inception = 4;
                           bytes nonce = 5; }
message WitnessConfig    { repeated bytes witnesses = 1; }  // EndpointIds
message TrustAttestation { bytes subject = 1; }
message TrustRevocation  { bytes target = 1; }        // attestation event_id
message OrgInvite  { bytes invitee = 1; bytes invitee_key = 2; Role role = 3;
                     bytes invitee_inception = 4; }
message OrgAcceptance { bytes acceptance = 1; bytes sig = 2; }
message OrgRemoval    { bytes target = 1; }
```

Decision: **an org event that names a person embeds that person's inception.**
`founder_inception` and `invitee_inception` carry the full `SignedEvent` bytes
of the person's seq-0 event. Verification requires its `event_id` to equal the
recorded `founder`/`invitee` id, requires it to verify standalone (canonical
form, self-consistency, `kind == PERSON`, valid self-signature), and requires
its `active_key` to equal the recorded `founder_key`/`invitee_key`. Rotation is
out of scope, so an inception's active key is authoritative for life. Org
ledgers are then self-contained and binding-checked: the id-to-key link is
proven in the chain, with no cross-ledger resolution and no "unresolved" verdict
for membership.

Decision: **organizations have no keys of their own**, which **amends
decisions/002 for organizations**: membership events, not a reserve key, provide
an org's change-of-authority path. decisions/004 has org actions signed by a
current controller's personal key, and custodying an org key would either give
one controller power outside the membership rules or need the group-key
machinery the digest drops. Decision: the only attestation type is
`TrustAttestation`, hearsay's `KnowsV1` without notes, scores or dates; the
subject may be a person or an org, and issuer and subject must differ.

Valid payloads by ledger kind. A person ledger holds `PersonInception` at seq 0,
self-signed by its `active_key`, then `WitnessConfig`, `TrustAttestation` and
`TrustRevocation`, all signed by that same active key. An org ledger holds
`OrgInception` at seq 0 signed by `founder_key`, then those same three plus
`OrgInvite`, `OrgAcceptance` and `OrgRemoval`, each signed by any current
controller, with `OrgAcceptance` additionally carrying the invitee's signature
inside. Every other combination is rejected.

"Any current controller" means `author_key` equals the active key the org ledger
recorded for an identity whose role is `CONTROLLER` in the state folded from
events `0..=i-1`; `OrgInception` seeds that state with the founder as
`CONTROLLER`. Decision: `MEMBER` grants no signing authority in this POC, being
recorded data only, so only controllers may append. **Field validation** below
is normative, runs after the wire-format validator and before any semantic rule,
and each rule gets a negative golden vector (section 11). Byte lengths are
exact.

| Field | Presence | Bytes | Rule |
|---|---|---|---|
| `SignedEvent.body` / `.sig` | required | <= 4096 / 64 | canonical `EventBody`; ed25519 over `sign_input` |
| `EventBody.version` | absent | - | v0 default; any other version rejected |
| `EventBody.ledger` / `.prev` / `.seq` | seq > 0 | 32 / 32 / - | equal `ledger_id`, `event_id(events[i-1])` and position `i` |
| `EventBody.timestamp_ms` | required | - | `1..=4102444800000`, non-decreasing |
| `EventBody.author_key` / `.payload` | required | 32 / - | key authorized by state `0..=i-1`; exactly one recognised variant |
| `*Inception.kind` / `.nonce` | required | - / 16 | kind matches the variant and sets the ledger kind |
| `PersonInception.active_key`, `.reserve_commit` | required | 32 | must differ |
| `OrgInception.founder`, `.founder_key`, `.founder_inception` | required | 32, 32, <= 1024 | id and key match the embedded standalone-valid PERSON seq-0 event |
| `WitnessConfig.witnesses` | 1 to 16 | 32 each | all distinct |
| `TrustAttestation.subject` | required | 32 | differs from `ledger_id` |
| `TrustRevocation.target` | required | 32 | unrevoked attestation earlier in this ledger |
| `OrgInvite.invitee`, `.invitee_key`, `.invitee_inception` | required | 32, 32, <= 1024 | as for `OrgInception` |
| `OrgInvite.role` | required | - | `MEMBER` or `CONTROLLER` |
| `OrgAcceptance.acceptance` / `.sig` | required | <= 1024 / 64 | canonical `Acceptance`; invitee signature over `accept_input` |
| `OrgRemoval.target` | required | 32 | a current member, controller or open invitee |
| `Acceptance.*` | `version` absent, rest required | 32 each | see 3.5 |

**Semantic rules.** `WitnessConfig` replaces the whole set. A
`TrustAttestation` is rejected if an unrevoked attestation for the same subject
exists, so "does A currently trust B" has one answer. `TrustRevocation.target`
must name an unrevoked `TrustAttestation` earlier in the same ledger, and
nothing is ever deleted (decisions/003-trust). The folded org state tracks every
invite as `open`, `accepted` or `cancelled`: a new `OrgInvite` is rejected only
if that invitee already has an `open` one, so re-inviting an existing member
with a different role is allowed and the matching acceptance updates the role,
which is the promotion path. `OrgRemoval.target` names an identity and cancels
its open invite and removes its membership, whichever exist, and must leave at
least one controller; self-removal is allowed under that constraint, and events
signed by a controller before removal stay valid.

#### 3.5 Org acceptance, the cross-signing case

The invitee does not hold the org ledger, cannot append to it, and does not know
what the head will be when the acceptance lands, so the acceptance is a detached
signed blob the org event embeds verbatim.

```protobuf
message Acceptance { uint32 version = 1; bytes org = 2; bytes invite_event = 3;
                     bytes invitee = 4; bytes invitee_key = 5; }
```

The invitee signs `accept_input` with their personal active key and returns the
bytes plus signature as an `AcceptanceFile` (3.8) or through the wallet UI; a
controller then appends `OrgAcceptance { acceptance, sig }`. Decision: embed the
roughly 140 bytes rather than reference them, so the org ledger stays
self-contained.

Verification requires all of: the blob passes the wire-format validator and the
field table; `org` equals this ledger's id; `invite_event` names an `open`
`OrgInvite` earlier in this ledger; `invitee` and `invitee_key` equal that
invite's fields, which its embedded inception already proved belong together;
`sig` verifies over `accept_input` under `invitee_key`; no earlier
`OrgAcceptance` on this branch references the same `invite_event`; and the outer
event is signed by a current controller. Those bindings make the acceptance
non-transplantable across organizations, invitations and identities, and the
ledger enforces single use (pitfall 4), branch-locally: the same acceptance can
appear on two divergent branches, which fork detection surfaces rather than
prevents (flag W, section 5).

#### 3.6 Verification rules

`mabel-core` exposes one function that folds an event sequence into a state. It
reads no local state and touches no disk, which makes cold verification a real
code path rather than an accident (pitfall 5).

**State boundary.** Event `i` is authorized and semantically checked against the
state folded from events `0` through `i-1` inclusive, the state *before* this
event, never the head state (pitfall 3). Its payload is applied only after every
structural, signature and policy check has passed. With rotation out of scope
this matters for org membership: an event signed by a controller removed at a
later sequence stays valid forever. For each event at position `i`:

1. Run the wire-format validator on the `SignedEvent` bytes, then on `body`.
2. Apply the field table (3.4), including `seq == i`. Positional indexing
   rejects duplicate and out-of-order sequence numbers (flag W, cheap half).
3. At `i == 0`: require an inception payload with `ledger` and `prev` absent,
   set `ledger_id = event_id` and the ledger kind from the inception variant,
   and seed the state (a person's active key and reserve commitment, or an org's
   founder as `CONTROLLER` with the embedded inception checked). Seq 0 is
   authorized by itself: the signature verifies under its own `active_key`, or
   under `founder_key` for an org.
4. At `i > 0`: require `ledger == ledger_id`, `prev == event_id(events[i-1])`,
   `timestamp_ms >= timestamp_ms(events[i-1])`, and a payload valid for the
   ledger kind (3.4).
5. Require `author_key` to be authorized by the state from `0..=i-1`, then
   verify the signature over `sign_input` using the received body bytes.
6. Check the payload's semantic rules against that same state, then apply it.

The folded state is the kind, the active key and reserve commitment (person),
the member and controller map plus the invite table (org), the witness set, the
trust map from attestation event id to subject and revocation status, and the
head. Decision: the fold returns `(state, Option<Violation>)`, the violation
carrying the failing sequence and reason and the state being the fold of the
valid prefix. Partial validity is reported, never accepted: `verify ledger`
prints `valid to seq N, failed at seq M: <reason>` and exits 20, and a witness
stores only the valid prefix (section 5).

#### 3.7 Cross-ledger resolution and multi-source verification

Org membership needs no cross-ledger lookup (3.4). A trust attestation still
names its subject by identity id alone, so resolving the subject means fetching
that ledger, verifying it from nothing, and requiring `ledger_id` to equal the
requested id; since the id is the inception digest, a tampered inception cannot
pass and no source is trusted. Decision: no address plumbing beyond
`EndpointId`, because Iroh's `N0` preset dials by id alone (research 002 section
6). Decision: if no source has it, verification still succeeds and reports
`subject: unresolved (not held by any queried source)`, since the subject's
participation is deliberately not required (decisions/003-trust).

Decision: with no `--from`, a verifier queries every configured witness in
parallel and verifies each candidate independently. A longer candidate wins only
if it extends the shorter one, event id for event id. Two valid candidates that
diverge at a sequence are equivocation: the verifier reports both source
endpoints and both event ids there and exits 20 rather than picking a winner.

#### 3.8 File artifacts

Three protobuf artifacts cross machines as files, each subject to the
wire-format validator, the field table and a size cap checked before allocation,
exactly as network input (pitfall 7).

```protobuf
message InviteBundle       { repeated SignedEvent org_prefix = 1; }  // <= 1 MiB
message AcceptanceFile     { bytes acceptance = 1; bytes sig = 2; }  // <= 4 KiB
message IdentityDescriptor { SignedEvent inception = 1;
                             repeated bytes witnesses = 2; }         // <= 64 KiB
```

`InviteBundle.org_prefix` holds org events `0..=invite`, so `org accept`
verifies the chain from inception, locates the named invite, displays a summary
of the org, its controllers and the offered role, and only then signs.
`IdentityDescriptor` is what `identity export` writes.

### 4. Keys

Three ed25519 key roles. The **identity active key** signs every event in a
person's ledger and the org events where that person is a controller; orgs have
none. The **identity reserve key** is generated at inception, stored beside it
and committed on-chain as `reserve_commit`, unused in the POC so rotation can
land later without changing ids. The **node key**, one per node home, is the
Iroh endpoint secret key and fixes the `EndpointId` witness configs reference.

Decision: `mabel-core` has no Iroh networking and no tokio, but it does depend
on low-level key types, primarily `iroh_base`'s, which avoids `ed25519-dalek`
twice in the graph (iroh pins it at `>=3.0.0-rc.0,<4.0.0`). Milestone 1 must
verify the exact feature name and constructor against the published crate before
anything is built on it; the current reading is `default-features = false,
features = ["key"]` with `SecretKey::from_bytes`. Fallback if that feature drags
in runtime dependencies: core depends on `ed25519-dalek` directly, inside iroh's
range. Application crates keep iroh's default features, including `portmapper`,
which only costs startup time in a container with no gateway to probe.

Decision: **the node key is distinct from every identity key.** A wallet holds
many identities and cannot make one of them its transport identity, and the node
key is a hot key used by a network listener; it signs no ledger content and
appears in no event except as an `EndpointId` inside a `WitnessConfig`.

Consequence, stated explicitly: **pushes are authorized by ledger content and by
the witness admission rule, not by transport identity.** A witness verifies
signatures and the chain and does not treat `connection.remote_id()` as
authorization, declining the free pusher authentication research 002 section 4
describes. Any peer may relay a ledger the witness already stores or that names
it in its own `WitnessConfig` (section 5), so a third party can help a peer
catch up, and nobody can push an invalid extension. Whoever pushes first pins
that branch under first-seen-wins, and a later divergent branch is recorded as a
fork rather than overwriting. `remote_id()` is provenance on stored events and
fork records and is shown in the debug UI, as evidence, never authorization.

### 5. Iroh sync protocol

ALPN `mabel/ledger/0`, one request per bidirectional stream, the server looping
on `accept_bi` so a wallet can push several ledgers over one connection. The
client writes the encoded request, calls `send.finish()` and reads the response
to EOF under a hard byte cap; the server mirrors that, with no length prefix
(research 002 section 5). Decision: **protobuf frames over raw Iroh bi-streams,
no gRPC**, so one schema language covers events and wire and
`proto/mabel/v0/sync.proto` sits beside `ledger.proto`.

```protobuf
message Request { oneof kind {   // tags 1 to 5
  Head { bytes ledger }   Get { bytes ledger; uint64 since; uint32 limit }
  Push { bytes ledger; repeated SignedEvent events }
  List { uint32 offset; uint32 limit }
  Forks { bytes ledger; uint32 offset; uint32 limit }  // empty ledger = all
}}
message Response { oneof kind {  // tags 1 to 7
  HeadResp { head_seq, head_event, updated_ms }
  EventsResp { repeated SignedEvent, head_seq, more }
  AcceptedResp { head_seq, stored }   LedgersResp { entries, more }
  ForksResp { entries, more }         NotFoundResp {}
  RejectedResp { RejectCode code; uint64 at_seq; string msg }
}}
```

`LedgerSummary` is `{ ledger, kind, head_seq, head_event, event_count,
first_seen_ms, updated_ms, fork_count, forks_truncated }`; `ForkRecord` is
`{ ledger, seq, SignedEvent kept, SignedEvent conflicting, observed_ms,
source_endpoint }`; `RejectCode` covers `MALFORMED`, `TOO_LARGE`, `INVALID`,
`FORK`, `UNSUPPORTED`, `NOT_ADMITTED` and `BUSY`.

**Caps**, enforced before any allocation sized by peer or file input (pitfall
7). Frames 4 MiB each way. Single event 4 KiB, roughly five times headroom,
since the largest realistic event is a 16-entry `WitnessConfig` at about 0.7 KiB
and an `OrgInvite` with an embedded inception about 0.5 KiB. `Push` at most 512
events and 2 MiB; `Get.limit` clamps to 512, `List.limit` to 256, `Forks.limit`
to 64. Per ledger 4096 events, 4 MiB and 8 fork records, after which
`forks_truncated` is set and recording stops; per witness 10000 ledgers and a
global storage cap defaulting to 2 GiB, configurable in `node.json`.
Concurrency: 32 connections, 64 requests per connection, 8 concurrent
verifications behind a semaphore, answering `BUSY` rather than queueing without
bound. Byte budgets are authoritative: a server fills a response to `min(count
limit, byte budget)` and sets `more`, and `List` orders by ascending ledger id
so paging is stable.

**Admission.** Decision: a witness accepts a `Push` only if the ledger is
already stored, or the pushed chain's folded `WitnessConfig` lists that
witness's own `EndpointId`; otherwise `Rejected { NOT_ADMITTED }`. Reads stay
open to all. This stops a witness being a free public dump while still letting a
third party relay a ledger that names it.

**Push semantics.** Pushed events must begin at seq 0 for a ledger the witness
does not hold, at `stored_head + 1`, or overlapping the stored suffix with
byte-identical events, which makes a retry idempotent. A gap is `Rejected
{ MALFORMED }`; a divergent event at a stored sequence takes the fork path; a
push valid up to some sequence and invalid after it has its valid prefix stored
atomically and answers `Rejected { INVALID }` with the failing `at_seq`.
Decision: a witness verifies a ledger fully from nothing once, at first ingest,
and keeps the folded state, rebuilt from disk on startup or on demand; later
pushes verify only the spliced suffix against it, which keeps a witness cheap
under repeated small appends. Full-chain-from-nothing verification remains the
CLI `verify` path and the fresh-verifier test, so the strict path stays
exercised.

**Fork records.** A conflicting event is recorded only if it fully verifies
against the shared prefix: canonical form, field table, sequence, ledger id,
authorized signer at that position, valid signature. Anything else is `Rejected
{ INVALID }` and is not stored. A `ForkRecord` carries both full `SignedEvent`s,
kept and conflicting, so a reader can check the conflict without a second
request, and `mabel-core` exposes the fork-record validation function the
witness and any reader share.

**Appending to a shared ledger.** An org ledger has several controllers, so
before appending, a wallet queries `Head` from the org's configured witnesses.
If any reports a head ahead of the local copy, the wallet fetches and
fast-forwards first. If a local unpushed event conflicts with an observed head,
the wallet discards it and re-signs the same intent on top of the new head, and
the CLI surfaces this as exit code 50, stale state. Losing a race is a retry.

**Flag W stance.** Witnesses stay passive and unsigned, with no receipts and no
thresholds (decisions/005-witnesses). Over accepting the gap, mabel adds
duplicate-sequence rejection inside a supplied log, refusal to overwrite the
first event seen at a sequence, fork records served through `Forks` and shown in
the debug UI, and multi-source comparison at verification time (3.7). A fork
record is self-authenticating although the witness signs nothing: it proves two
distinct validly signed events exist at one sequence, produced by whoever held
signing authority there, which is evidence of equivocation or of a lost race
between honest controllers, and the output says that and nothing further. Still
missing: nothing forces a witness to report a fork.

### 6. Stances on the digest flags

**W, equivocation.** Per section 5: no receipts, but duplicate-sequence
rejection, first-seen-wins, self-verifying fork records and multi-source
comparison, with the residual gap documented rather than silently dropped.

**A, anchoring versus inlining.** Payloads are inlined, with no separate record
digest and no seal anchoring, which is acceptable because ledgers are public.
Lost: proving one attestation hands over the ledger prefix, so the verifier sees
the issuer's trust graph to that point; there is no portable single-record file;
and payload bytes are no longer stable independently of ledger framing.

**P, exact-payload binding.** The mechanism survives the policy: a controller's
signature covers the exact encoded bytes of the org event that lands in the
ledger, not a digest of a proposal, and the invitee's covers the exact embedded
acceptance bytes.

**L, liveness proof of the subject.** Out of scope, no challenge-response
(decisions/008); the issuer is responsible for out-of-band confirmation that the
subject controls the identity. The README says so and verifier output includes
`subject control was not proven to this verifier; the issuer is responsible for
out-of-band confirmation`.

**R, revocation completeness.** A verifier reports what it checked and where it
came from, never global completeness: `valid as of seq N of <ledger id>, fetched
from <EndpointId> at <time>; no revocation up to seq N`, never "unrevoked".
Every `--json` result carries `source`, `head_seq`, `head_event`, `fetched_at`.

**D, discovery.** No global discovery and no "who trusts B" query, since
attestations live only in the issuer's ledger; the debug UI and `List` enumerate
what one witness holds, a diagnostic rather than an index.

### 7. Repository and crate layout

Cargo workspace, MSRV 1.91, edition 2024 (iroh's floor).

```
proto/mabel/v0/{ledger,sync,files}.proto  normative schemas
crates/mabel-proto/  prost-generated types only, build.rs, no logic
crates/mabel-core/   ledger semantics, canonical encoding, wire-format
                     validator, digests, verification, fold, fork-record
                     validation. No iroh networking, no tokio, no filesystem
crates/mabel-net/    client and ProtocolHandler server, caps, address lookup
crates/mabel-node/   node home, storage, key files and permissions, wallet and
                     witness runtimes, sync, axum API, UI serving
crates/mabel-cli/    the `mabel` binary, clap, output rendering, exit codes
ui/                  one Vite app: two routes, shared components, one bundle
tests/e2e/           Playwright specs; test-vectors/ golden bytes and digests
docker/              Dockerfile and the compose demo topology
```

Decision: a separate `mabel-proto` so exactly one crate runs `protoc` and core
and net share the generated types. Decision: storage lives in `mabel-node`,
being a few hundred lines of file IO with no other consumer. Decision: core
stays async-free and IO-free, so wasm and mobile remain reachable and
verification cannot read local state (pitfall 5).

### 8. Storage

One home per node, `$MABEL_HOME` or `~/.mabel`, overridable with `--home`.

```
node.json                       role, http bind, witnesses, storage cap
node.key                        0600, iroh endpoint secret key
identities/<id>/meta.json       alias, kind, created_at (never signed)
identities/<id>/{active,reserve}.key   0600
ledgers/<id>/000000000000.ev    encoded SignedEvent, one file per event
ledgers/<id>/head.json          cache: seq, event id, updated_ms (rebuildable)
ledgers/<id>/meta.json          provenance: source endpoint, first seen
forks/<id>/<seq>-<event_id>.fork  encoded ForkRecord, both events
peers.json                      ledger id to EndpointId hints, plus tickets
```

Decision: plain files, no database. Event files are named by zero-padded
sequence so directory order is chain order, and the only access patterns, "read
all" and "read from seq N", are served by a sorted listing. One encoded
`SignedEvent` per file also keeps byte authority honest: the file is the signed
object and is served to peers unmodified. Writes are atomic (temp file, fsync,
rename), and a multi-event append renames `head.json` last so a crash leaves a
shorter but valid ledger. Permissions come verbatim from hearsay: directories
0700, key files 0600, and a group- or world-readable key file fails with exit
code 60 unless `--allow-insecure-permissions` is passed.

### 9. CLI surface

Global flags: `mabel [--home PATH] [--json] [--verbose]
[--allow-insecure-permissions] <command>`. Every command supports `--json` with
a stable document, and JSON errors are `{ok, code, message, details}`. Aliases
are local and never signed; ids are authoritative.

```
identity create --alias <a> | list | show <alias|id> | export <alias|id>
trust add|revoke|list --issuer <alias|id> [--subject <id>|--attestation <id>]
org create --alias <a> --founder <alias|id> | show <alias|id>
org invite --org <id> --by <id> --invitee <descriptor-file>
           --role member|controller --out <invite-bundle>
org accept <invite-bundle> --as <alias|id> --out <acceptance-file>
org admit  --org <id> --by <id> <acceptance-file>
org remove --org <id> --by <id> --member <id>
witness add --identity <alias|id> --endpoint <endpoint-id>
sync push --identity <alias|id> [--to <id>] | sync fetch <ledger-id> --from <id>
verify ledger <ledger-id> | verify trust --issuer <id> --subject <id>
witness run [--http <addr>] [--iroh-port <n>] | wallet serve [--http <addr>]
node id
# network commands also take --from <endpoint-id> to pin one source and
# --peer <ticket> to seed address lookup
```

Decision: `org invite` takes the invitee's `IdentityDescriptor` file rather than
a raw id and key, because the invite must embed the invitee's inception (3.4).
Decision: `org invite` / `org accept` / `org admit` replaces hearsay's
invite/accept/finalize; two parties sign, so three steps remain, but "admit"
names what the third does. Decision: `verify trust` has a pinned answer, the
same in text and `--json`. If a single unrevoked attestation for that subject
exists in `0..=head`, the result is `trusted: true` with its event id and
sequence; otherwise `trusted: false` with the count and event ids of revoked
attestations. Both exit 0, because "not trusted" is a successful verification.

Exit codes, hearsay's table minus code 40 (pending approvals, dropped with
proposals): 0 success, 2 usage, 10 invalid schema or malformed input, 20
cryptographic, semantic or equivocation failure, 30 peer or network unavailable,
50 stale state or conflicting event or replay, 60 insecure key file permissions,
70 unsupported feature. Errors name their layer as hearsay's do: `Schema
error:`, `Ledger error:`, `Policy error:`, `State error:`, `Replay error:`,
`Network error:`.

### 10. Web UIs

The node serves an axum HTTP JSON API on loopback plus static UI assets. All
logic lives in the node; the UI calls the API, holds no keys and does no crypto.
Decision: **React 19 + Vite + TypeScript + Tailwind + shadcn/ui**, as one app
with two routes, wallet and witness, sharing components in one source tree and
building to one bundle, embedded with `rust-embed` and served from disk with
`--ui-dir` in development. shadcn gives a clean look with no design work,
Playwright drives it through stable `data-testid` attributes, and one app avoids
a second build pipeline for what is a diagnostics page.

Wallet API under `/api`: `GET /node`; `GET|POST /identities`; `GET
/identities/:id` and `/identities/:id/ledger?since=`; `POST
/identities/:id/witnesses`; `POST /trust` and `/trust/:event_id/revoke`; `POST
/orgs`, `/orgs/:id/invites`, `/orgs/:id/acceptances`, `/orgs/:id/removals`;
`POST /sync/push`; `POST /verify`. Witness API, read-only: `GET /node`,
`/ledgers`, `/ledgers/:id`, `/ledgers/:id/events?since=` and `/forks`.

Decision: no authentication, but three loopback rules in one axum middleware
layer: reject any request whose `Host` is not `127.0.0.1` or `localhost` with
the expected port; reject mutating requests whose `Origin` does not match that
host; require `content-type: application/json` on mutating routes. That blocks
DNS rebinding and drive-by form posts from a page the user happens to have open,
the realistic threat to a keyholding daemon on loopback. Both servers bind
`127.0.0.1` by default and binding elsewhere prints a warning. UI verification
results use the same "as of seq N from source S" struct as the CLI.

### 11. Testing strategy

- **Core unit tests**, one negative case per rule. Validator: unknown field
  number, duplicate non-repeated field, out-of-order fields, non-minimal varint,
  wrong wire type, unrecognised `oneof` variant, `*_UNSPECIFIED` enum. Field
  table: every presence, length, uniqueness and cross-field rule, including an
  `OrgInvite` whose embedded inception does not hash to `invitee` and one whose
  `active_key` differs from `invitee_key`. Chain: broken prev link, duplicate
  sequence, gap, wrong ledger id, backwards timestamp, timestamp past the
  year-2100 bound, unauthorized signer, payload wrong for the ledger kind, a
  `MEMBER` signing. Policy: acceptance replayed to another org, to another
  invite and twice to one invite, invite over an open invite, re-invite plus
  acceptance promoting a member, removal of the last controller, removal
  cancelling an open invite, revocation of an unknown or revoked attestation,
  and fork-record validation taking a real conflict while rejecting a malformed
  or unauthorized one.
- **Golden vectors** in `test-vectors/`: per event type the encoded body bytes
  as hex, the event id, the signature under a fixed key and a JSON rendering,
  plus one rejection vector per validator and field-table rule. Tests assert the
  encoder reproduces bytes and digests exactly (pitfall 1) and that flipping one
  byte fails. These double as the conformance suite for non-Rust clients.
- **Protocol tests** with two in-process endpoints, `presets::Minimal` and
  `RelayMode::Disabled`, dialling the loopback `EndpointAddr` (research 002
  section 8, level 1): every request type, oversize, truncated and garbage
  input, gapped push (`MALFORMED`), idempotent overlapping re-push, partially
  invalid push storing the valid prefix and naming `at_seq`, push for an unknown
  ledger not naming the witness (`NOT_ADMITTED`), paging and byte budgets, and a
  fork push producing a `ForkRecord` with both events while the first survives.
- **CLI tests** with `assert_cmd` and a temp home: every command and exit code,
  `--json` shape stability, `verify trust` trusted, revoked and
  unresolved-subject cases, stale append against a moved head (exit 50),
  insecure permissions (exit 60), file-artifact caps. **API tests** for the
  three loopback rules: bad `Host`, mismatched `Origin`, missing content type.
  **Fresh-verifier test**: wipe the home, then `verify trust` against a witness
  with no local state and no keys, hearsay's best acceptance test.
- **Playwright e2e** over the compose topology, driving both UI routes and the
  CLI in one scenario: two people create identities in two wallets, configure
  the witness, push, one founds an org and invites the other, the invitee
  accepts in their own wallet UI, the founder admits, the person and the org
  each attest trust in a third identity, a stranger verifies from an empty home,
  the issuer revokes, and the witness route shows the ledgers and heads. A
  second scenario forces a fork, asserts the UI shows the conflict and keeps the
  first event, and asserts a verifier querying two witnesses on divergent
  branches reports both sources and exits 20.
- **Containers**: a multi-stage Dockerfile (rust build, node UI build, slim
  runtime) producing one image for both roles, plus `docker/compose.yaml` with
  two wallets and one witness, keys on volumes, fixed UDP ports, wallet HTTP
  ports exposed for Playwright, and the witness `EndpointTicket` seeded into
  each wallet's `peers.json` so the suite needs no internet.

### 12. Dependencies

Rust, checked against crates.io on 2026-08-23: `iroh` 1.0.3, `iroh-base` 1.0.3
(key types in core, feature confirmed at milestone 1), `iroh-tickets` 1.0.0
(tickets for `--peer` and compose seeding), `prost` 0.14.4 with `prost-build`
0.14.4 and `protoc-bin-vendored` 3.2.0 so no system `protoc` is needed, `blake3`
1.8.7, `data-encoding` 2.11.1 for base32 ids, `thiserror` 2.0.20, `serde`
1.0.229 and `serde_json` 1.0.151 (HTTP API, config, vectors), `tokio` 1.53.1
(`rt-multi-thread, macros, fs, signal, sync`), `axum` 0.8.9, `tower-http` 0.7.0,
`rust-embed` 8.12.0, `clap` 4.6.6, `tracing` 0.1.44 with `tracing-subscriber`
0.3.23, `anyhow` 1.0.104 (node and cli only), `getrandom` 0.4.3. Dev: `iroh`
with `test-utils`, `tempfile` 3.27.0, `assert_cmd` 2.2.2. Keys are 32 bytes from
`getrandom` passed to `SecretKey::from_bytes`, sidestepping the `rand_core`
version dance; the section 4 fallback would add `ed25519-dalek` to core inside
iroh's range. Frontend, checked against npm the same day: `react` 19.2.8, `vite`
8.2.2, `tailwindcss` 4.3.3, `typescript` 7.0.2, `@playwright/test` 1.62.1,
shadcn/ui vendored into the single `ui/` app, Node 22 LTS in the build stage.

### 13. Milestones

1. Workspace, `.proto` schemas, `mabel-proto`, canonical encoding, wire-format
   validator, digests, golden and rejection vectors, and confirmation of the
   `iroh-base` key feature and constructor.
2. Person ledger and the fold: inception, witness config, attestation,
   revocation, exclusive state boundary, partial-validity result.
3. Org ledger: inception and invite with embedded inceptions, acceptance,
   admit, removal, invite lifecycle, replay and promotion tests.
4. Node home and storage: layout, atomic writes, permissions, head cache, file
   artifacts and caps.
5. CLI for everything local: exit codes, `--json` shapes, pinned `verify trust`.
6. `mabel-net`: ALPN, sync schema, caps and byte budgets, `MemoryLookup` and
   tickets, client and `ProtocolHandler`, two-endpoint loopback tests.
7. Witness runtime: admission, incremental suffix verification, push semantics,
   fork records, paging, `witness run`, witness API and UI route.
8. Wallet runtime: `wallet serve`, HTTP API with the loopback rules, wallet UI
   route, sync, append discipline (exit 50), multi-source verification.
9. Containers: Dockerfile stages, compose topology with seeded tickets, demo
   script.
10. Playwright e2e including the fresh verifier, the fork case and the
    multi-source equivocation assertion, then docs and gap analysis against the
    decision records.

## Alternatives considered

- **Postcard for event bytes**: canonical by construction and smallest, but
  effectively Rust-only, which fails the planned iOS, Android and web clients.
  **Canonical JSON with sorted keys**: readable and hearsay-shaped, but it
  carries every canonicalization trap and gives no language-neutral schema.
- **`encoded_len` as the strict-parsing gate**: cheap, but it accepts a
  reordered encoding of equal length, so a byte scanner replaced it. **A single
  `Event` message with an inline signature** (decisions/007's sketch): needs a
  placeholder rule to sign a message containing its own signature.
- **Resolving a controller's or invitee's ledger at verification time** instead
  of embedding their inception: makes org membership depend on network
  reachability and yields an "unresolved" verdict for a governance fact.
- **gRPC or tonic over Iroh**: HTTP/2 framing and a transport adapter on a
  transport that already multiplexes and authenticates, for five request types.
- **iroh-blobs, iroh-gossip, iroh-docs**: content addressing cannot express
  "everything after seq N" for a mutable head; gossip is unordered best-effort
  delivery to a membership set that must be bootstrapped and answers no read
  path; docs is a replicated key-value store with its own authorship model that
  duplicates the ledger mabel writes anyway (research 002 section 7).
- **A static allowlist of pushing endpoints**: operator configuration that the
  on-ledger `WitnessConfig` already expresses, so admission reads the chain.
  **Witness receipts and a threshold**, hearsay's equivocation answer: ruled out
  by decisions/005-witnesses, with fork records and multi-source comparison as
  the passive substitute.
- **askama plus htmx**: less code and no node toolchain, but the owner asked for
  a well-known framework, and the JSON API is needed regardless. **Two UI
  bundles**: a second build pipeline for a diagnostics page. **An embedded
  database**: no index exists that a sorted directory listing does not serve.
- **Reusing an identity key as the node key** for free pusher authentication:
  ties transport identity to one of a wallet's many identities and exposes a
  signing key to a listener, while the chain already authenticates content.

## Clarifications

Added 2026-08-24 after ticket cutting surfaced ambiguities; these rulings are
part of the accepted proposal.

- Size caps: the 4096-byte event cap applies to the encoded `SignedEvent`;
  `EventBody` fits inside it. An embedded inception (`founder_inception`,
  `invitee_inception`) is capped at 1024 bytes; real inceptions are ~150.
- `test-vectors/` lives at the repository root.
- The `.proto` files under `proto/mabel/v0/` are the field-number authority
  for sync messages; the sketch in section 5 is illustrative.
- `Get.since` is inclusive: the response starts at `seq == since`.
- An unresolved trust subject exits 0; only chain, signature or equivocation
  failures exit 20.
- `node.json` gets a `relay` setting: `"n0"` (default) or `"disabled"`. The
  compose topology sets `"disabled"` and seeds `EndpointTicket`s, which is
  what makes the container suite runnable with no internet.
- Fork record files are named by the conflicting event's id; the kept event
  already lives in the ledger directory.
- Orgs surface through `GET /identities` (an org is an identity with kind
  `org`); there is no separate `GET /orgs`.
- Ids use literal RFC 4648 base32, lowercase display, case-insensitive
  parse. iroh-base 1.0.3 displays keys as hex, so "Iroh's alphabet" in 3.1
  was inaccurate; the encoding stands on its own.

## Consequences

Easier: one fold function serves the CLI, the wallet, the witness and the
fresh-verifier test, because core does no IO. Org ledgers are self-contained, so
membership verification never depends on reaching another node. The `.proto`
files, the canonical-encoding prose and the golden vectors are a complete
implementation contract for a non-Rust client, and the byte-scanning validator
closes the equal-length-encoding hole an `encoded_len` check would have left.

Harder: byte authority is a discipline the compiler cannot fully enforce, so
every path that touches an event carries bytes rather than structs. The
wire-format validator is hand-written work tested rule by rule, and any second
implementation must emit the canonical form exactly. Embedding inceptions makes
org events several hundred bytes larger and copies a person's inception into
every org naming them, and inlining payloads means proving one attestation
discloses the issuer's prefix.

Deferred: key rotation, which the embedded-inception rule assumes away by
treating an inception's active key as authoritative for life; equivocation
defense beyond first-seen-wins and multi-source comparison; subject liveness
proof; discovery of who trusts whom; wasm and mobile builds; and authentication
beyond the loopback rules.
