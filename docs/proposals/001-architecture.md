# 001: mabel architecture

- Date: 2026-08-23
- Status: proposed
- Decisions affected: decision 001, 002, 003, 004, 005, 006, 007, 008

## Context

Mabel must reproduce hearsay's claim (a person or an organization can make a
signed, independently verifiable statement that it personally knows someone) on
a minimal hash-chained ledger plus Iroh, without KERI. The decision records fix
the product shape and leave open the event encoding, the wire protocol, the
crate cut, storage and the UI stack. This proposal settles those precisely
enough to cut into tickets.

Inputs: `docs/decisions/` (authoritative, cited as `decisions/NNN-name`),
`docs/research/001-hearsay-digest.md` (flags W, A, P, L, R, D and pitfalls 1 to
8, cited by number), `docs/research/002-iroh-research.md`. Judgment calls are
marked "Decision:" so reviewers can target them.

## Proposal

### 1. Goals and non-goals

Goals: one Rust core library powering every node type with no networking in it;
one append-only, hash-chained, ed25519-signed ledger per identity, verifiable
from nothing; person and org identities, one-way trust attestations and
revocations, org membership by invitation plus signed acceptance; replication
over Iroh to passive witnesses; a `mabel` CLI, a wallet UI, a witness debug UI,
container images, and end-to-end tests driving all three (decisions/001-scope).

Non-goals (decisions/008-out-of-scope): key rotation, witness receipts and
thresholds, challenge-response proof of control, multi-approval org governance,
encrypted key storage, backups, wasm and mobile builds.

Ledgers are public, replicated data. Every event, including every trust
attestation and revocation, is visible to anyone who can name the ledger id.
Partial disclosure is a non-goal: mabel has no way to prove one attestation
without handing over the ledger prefix containing it. Private keys never leave
the node home; everything else is publishable.

Verified means "this identity signed this statement at this position in its
chain". It is not proof that the statement is true, not proof of legal identity,
and not proof of unique humanity. That sentence goes in verifier output, the
README and this proposal (pitfall 8).

### 2. System overview

Two node roles, one binary.

A **wallet node** holds private keys for many identities (persons and orgs),
appends events to the ledgers it controls, pushes them to witnesses, fetches
other ledgers to verify claims, and serves an HTTP API plus the wallet UI on
loopback. It also serves the Iroh protocol read-only so peers can fetch its
ledgers directly.

A **witness node** is a passive replica (decisions/005-witnesses). It holds full
copies of many ledgers it does not control, accepts pushes from anyone, verifies
every chain before storing, serves reads, records forks, and exposes a debug UI
listing what it holds. It signs nothing and holds no identity keys.

Wallets, witnesses and verifiers talk over Iroh, ALPN `mabel/ledger/0`, with
protobuf frames on one bidirectional stream per request (section 5). Browsers
talk HTTP JSON to the node on loopback. Humans use the `mabel` CLI against the
node home on disk. There is no application server and no database
(decisions/006-networking).

### 3. Ledger specification

#### 3.1 Event encoding and byte authority

Decision: **protobuf (proto3) encoded with prost**, per decisions/007. Postcard
is canonical by construction but effectively Rust-only, and canonical JSON
carries every canonicalization trap hearsay warns about. Protobuf wins on the
requirement neither meets: the owner plans iOS, Android and web clients, and a
checked-in `.proto` is the schema they build from, which also turns the golden
vectors into a cross-language conformance suite.

Protobuf is not canonical: field order, varint padding and packing all vary.
Mabel does not try to fix that. **The encoded bytes are authoritative.** A
signer serializes once, and those exact bytes are hashed, signed, stored and
shipped; a verifier hashes and checks the bytes it received and decodes only to
read fields. Verification never re-serializes, which is also the only rule that
survives a second protobuf implementation.

Three consequences the implementation must respect. Any re-encoding, however
neutral, invalidates the signature; the bytes are the object. A signer can
encode one logical event two ways and sign both, which is equivocation at one
sequence and is caught as a fork (section 5). Digest stability becomes a storage
discipline: nothing may decode-then-encode an event, so `SignedEvent` travels as
bytes end to end and only the signing path produces them (pitfall 1).

Decision: strict parsing (pitfall 2) is a length check, not a re-serialization,
because prost silently drops unknown fields. After decoding, require
`prost::Message::encoded_len() == received.len()`, which computes the canonical
length without producing bytes, so a dropped unknown field or a padded varint
fails. Reordered fields keep the length and pass, which is correct: an attacker
cannot reorder without breaking the signature. Unrecognised `oneof` variants and
`*_UNSPECIFIED` enum values are rejected.

Decision: BLAKE3-256 everywhere, matching hearsay, with domain separation:

```
event_id       = BLAKE3(b"mabel/event/v0\n"   || event_body_bytes)
sign_input     =        b"mabel/sig/v0\n"     || event_body_bytes
accept_input   =        b"mabel/accept/v0\n"  || acceptance_bytes
reserve_commit = BLAKE3(b"mabel/reserve/v0\n" || reserve_public_key)
```

Ids display as lowercase RFC 4648 base32 without padding (52 characters), the
alphabet Iroh uses for `EndpointId`; the type is known from context, so there is
no prefix. `proto/mabel/v0/ledger.proto` is normative, field numbers and `oneof`
tags are append-only, and a breaking change means a `v1` directory, a new
envelope `version` and a new ALPN.

#### 3.2 Event envelope

```protobuf
message EventBody {
  uint32 version      = 1;  // 0
  bytes  ledger       = 2;  // 32 bytes; empty at seq 0
  uint64 seq          = 3;  // 0-based, strictly +1 per event
  bytes  prev         = 4;  // previous event_id; empty at seq 0
  uint64 timestamp_ms = 5;  // unix milliseconds, advisory
  bytes  author_key   = 6;  // 32-byte ed25519 public key
  oneof payload {
    PersonInception  person_inception  = 10;
    OrgInception     org_inception     = 11;
    WitnessConfig    witness_config    = 12;
    TrustAttestation trust_attestation = 13;
    TrustRevocation  trust_revocation  = 14;
    OrgInvite        org_invite        = 15;
    OrgAcceptance    org_acceptance    = 16;
    OrgRemoval       org_removal       = 17;
  }
}
message SignedEvent {
  bytes body = 1;  // the exact encoded EventBody bytes
  bytes sig  = 2;  // 64-byte ed25519 signature over sign_input
}
```

Decision: two messages, not the single `Event` message decisions/007 sketches. A
signature cannot sit inside the message it signs without a placeholder rule, and
an opaque `bytes` body makes byte authority mechanical: no decoder is tempted to
normalize a length-delimited blob.

Decision: no separate key id space, so `author_key` is the public key itself and
key resolution is a set membership test. Decision: `ledger` and `prev` are empty
at seq 0 rather than zero-filled, since proto3 omits empty `bytes`; verification
requires length 0 at seq 0 and 32 elsewhere. Timestamps are advisory:
verification requires only that `timestamp_ms` is non-decreasing.

#### 3.3 Identity id derivation

`identity_id = ledger_id = event_id of the seq-0 event`, so naming an identity
commits to its exact inception bytes, including the active key and the reserve
commitment, and an inception from an untrusted source is checked by recomputing
the digest.

Both inceptions carry an explicit `kind` field (`PERSON` or `ORG`). The `oneof`
tag already separates them, but the explicit field is what the no-collision rule
rests on and it survives re-encoding (pitfall 6; hearsay worked around this by
permuting witness order). Inceptions also carry a 16-byte `nonce`, since two
orgs founded by the same controller in the same millisecond would otherwise
share an id; later events need none.

#### 3.4 Payload types

```protobuf
enum IdentityKind { IDENTITY_KIND_UNSPECIFIED = 0; PERSON = 1; ORG = 2; }
enum Role         { ROLE_UNSPECIFIED = 0; MEMBER = 1; CONTROLLER = 2; }

message PersonInception  { IdentityKind kind = 1; bytes active_key = 2;
                           bytes reserve_commit = 3; bytes nonce = 4; }
message OrgInception     { IdentityKind kind = 1; bytes founder = 2;
                           bytes founder_key = 3; bytes nonce = 4; }
message WitnessConfig    { repeated bytes witnesses = 1; }  // EndpointIds, max 16
message TrustAttestation { bytes subject = 1; }
message TrustRevocation  { bytes target = 1; }         // attestation event_id
message OrgInvite  { bytes invitee = 1; bytes invitee_key = 2; Role role = 3; }
message OrgAcceptance { bytes acceptance = 1; bytes sig = 2; }
message OrgRemoval    { bytes target = 1; }
```

Decision: **organizations have no keys of their own.** decisions/002-ledger says
inception creates an active and a reserve key, but decisions/004-organizations
says org actions are signed by a current controller's personal key, and
custodying an org key would either give one controller power outside the
membership rules or need the group-key machinery the digest drops. An org
inception records the founder's identity id and active key and is signed by that
key. Decision: the only attestation type is `TrustAttestation`, hearsay's
`KnowsV1` without notes, scores, dates or confidence; the subject may be a
person or an org, and issuer and subject must differ.

| Payload | Person ledger | Org ledger | Signer |
|---|---|---|---|
| `PersonInception` | seq 0 only | never | its own `active_key`, self-signed |
| `OrgInception` | never | seq 0 only | `founder_key` |
| `WitnessConfig` | yes | yes | active key / any current controller |
| `TrustAttestation` | yes | yes | active key / any current controller |
| `TrustRevocation` | yes | yes | active key / any current controller |
| `OrgInvite` | never | yes | any current controller |
| `OrgAcceptance` | never | yes | a current controller, plus the invitee inside |
| `OrgRemoval` | never | yes | any current controller |

"Any current controller" means `author_key` equals the active key the org ledger
recorded for an identity whose role is `CONTROLLER` in the state folded from
events `0..seq`. Because admission records that key, checking signatures on an
org ledger needs no other ledger; resolving a controller's identity still needs
theirs, a separate optional check (section 3.7).

Semantic rules. `WitnessConfig` replaces the whole set, at most 16 entries. A
`TrustAttestation` is rejected if an unrevoked attestation for the same subject
exists, so "does A currently trust B" has one answer. `TrustRevocation.target`
must name an unrevoked `TrustAttestation` earlier in the same ledger, and
nothing is ever deleted (decisions/003-trust). An `OrgInvite` is rejected if the
invitee is already a member or already has an open invite. An `OrgRemoval` must
name a current member or controller and must leave at least one controller;
self-removal is allowed under that constraint, and events signed by a controller
before removal stay valid (section 3.6).

#### 3.5 Org acceptance, the cross-signing case

The invitee does not hold the org ledger, cannot append to it, and does not know
what the head will be when the acceptance lands, so the acceptance is a detached
signed blob the org event embeds verbatim.

```protobuf
message Acceptance { uint32 version = 1; bytes org = 2; bytes invite_event = 3;
                     bytes invitee = 4; bytes invitee_key = 5; }
```

The invitee signs `accept_input` with their personal active key and returns the
bytes plus signature as a small file or through the wallet UI; a controller then
appends `OrgAcceptance { acceptance, sig }`. Decision: embed the bytes rather
than reference them, so the org ledger stays self-contained; the blob is about
110 bytes.

Verification requires all of: the blob decodes, passes the `encoded_len` gate
and has `version == 0`; `org` equals this ledger's id; `invite_event` names an
`OrgInvite` earlier in this ledger; `invitee` and `invitee_key` equal that
invite's fields; `sig` verifies over `accept_input` under `invitee_key`; no
earlier `OrgAcceptance` references the same `invite_event` and the invite was
not superseded by an `OrgRemoval`; and the outer event is signed by a current
controller. Those bindings make the acceptance non-transplantable across
organizations, invitations and identities, and the ledger enforces single use,
so hearsay's local marker file is unnecessary (pitfall 4). Binding id plus key
is the rotation-free equivalent of hearsay binding the invitee's establishment
event digest, since `invitee` is the digest of an inception that commits to that
key.

#### 3.6 Verification rules

`mabel-core` exposes one function that folds a full event sequence into a state
and fails at the first violation. It reads no local state and touches no disk,
which is what makes cold verification a real code path rather than an accident
(pitfall 5). For each event at position `i`:

1. Decode `SignedEvent`, require `encoded_len == len` and a 64-byte `sig`.
2. Decode `EventBody` from `body`, require `encoded_len == body.len()`,
   `version == 0` and a recognised payload.
3. Require `seq == i`. Positional indexing rejects duplicate and out-of-order
   sequence numbers (flag W, cheap half).
4. At `i == 0` require empty `ledger` and `prev` and an inception payload, and
   set `ledger_id = event_id`; otherwise require `ledger == ledger_id` and
   `prev == event_id(events[i-1])`.
5. Require `timestamp_ms >= timestamp_ms(events[i-1])`.
6. Require the payload is valid for the ledger kind (table in 3.4).
7. Require `author_key` is authorized **by the state folded from events
   `0..i`**, never by the head state (pitfall 3). With rotation out of scope
   this matters for org membership: an event signed by a controller removed at a
   later sequence stays valid forever.
8. Verify the signature over `sign_input` using the received body bytes.
9. Apply the payload's semantic rules and update the state.

The folded state is the kind, the active key and reserve commitment (person),
the member and controller map from identity to role and key (org), the witness
set, the trust map from attestation event id to subject and revocation status,
and the head. Verification never partially accepts: a chain failing at seq 7 is
valid only to seq 6, and the CLI says exactly that.

#### 3.7 Cross-ledger resolution

A trust attestation names its subject by identity id and nothing else, so
verifying it needs only the issuer's ledger while resolving the subject needs
the subject's. The verifier fetches that ledger by id from `--from
<EndpointId>`, else the configured witnesses, else a cached hint; verifies the
chain from nothing; and requires `ledger_id` to equal the requested id. Since
the id is the inception digest, a tampered inception cannot pass, so no source
is trusted. Decision: no address plumbing beyond `EndpointId`, because Iroh's
`N0` preset dials by id alone (research 002 section 6), so a witness list is a
list of 52-character strings. Decision: if no source has the subject's ledger,
verification still succeeds and reports `subject: unresolved (not held by any
queried source)`, since the subject's participation is deliberately not required
(decisions/003-trust).

### 4. Keys

Three ed25519 key roles. The **identity active key** signs every event in a
person's ledger and the org events where that person is a controller; orgs have
none. The **identity reserve key** is generated at inception, stored beside the
active key and committed on-chain as `reserve_commit`, unused in the POC so that
rotation can land later without changing ids (decisions/002-ledger). The **node
key** is one per node home: it is the Iroh endpoint secret key and fixes the
`EndpointId` that witness configs reference.

Decision: use `iroh_base::SecretKey` and `PublicKey` for all three rather than
depending on `ed25519-dalek` directly, since iroh pins dalek at
`>=3.0.0-rc.0,<4.0.0` and a direct dependency risks two incompatible
`SigningKey` types in one graph. `iroh-base` with `default-features = false,
features = ["key"]` pulls no tokio and no iroh runtime, so core stays portable;
if it proves awkward, core defines 32-byte key newtypes behind `Signer` and
`Verifier` traits that `mabel-net` implements.

Decision: **the node key is distinct from every identity key.** A wallet holds
many identities and cannot make one of them its transport identity, and the node
key is a hot key used by a network listener. It signs no ledger content and
appears in no event except as an `EndpointId` inside a `WitnessConfig`.

Consequence, stated explicitly: **pushes are authorized by ledger content, not
by transport identity.** A witness verifies signatures and the chain and does
not check `connection.remote_id()`, declining the free pusher authentication
research 002 section 4 describes. So anyone may relay anyone's ledger to any
witness, which is the desired replication behaviour; nobody can push an invalid
extension; a witness cannot account by owner, so it relies on size caps,
per-connection request caps and an optional allowlist for demos; and a hostile
relayer arriving first with a valid but abandoned fork pins it under
first-seen-wins, which is the equivocation case the witness exists to surface.
`remote_id()` is recorded as provenance on stored events and fork records and
shown in the debug UI, as evidence, never as authorization.

### 5. Iroh sync protocol

ALPN `mabel/ledger/0`, one request per bidirectional stream, the server looping
on `accept_bi` so a wallet can push several ledgers over one connection. The
client writes the encoded request, calls `send.finish()` and reads the response
to EOF under a hard byte cap; the server mirrors that. No length prefix, per
research 002 section 5.

Decision: **protobuf frames over raw Iroh bi-streams, no gRPC.** tonic would
layer HTTP/2 framing and a transport adapter onto a stream transport that
already multiplexes and authenticates, for five request types. Protobuf frames
keep one schema language across events and wire, so `proto/mabel/v0/sync.proto`
sits beside `ledger.proto` and a future Swift or Kotlin client generates both.

```protobuf
message Request { oneof kind {
  Head  head  = 1;  // { bytes ledger }
  Get   get   = 2;  // { bytes ledger; uint64 since; uint32 limit }
  Push  push  = 3;  // { bytes ledger; repeated SignedEvent events }
  List  list  = 4;  // { uint32 offset; uint32 limit }
  Forks forks = 5;  // { bytes ledger }  empty means all
}}
message Response { oneof kind {
  HeadResp     head      = 1;  // { head_seq, head_event, updated_ms }
  EventsResp   events    = 2;  // { repeated SignedEvent, head_seq, more }
  AcceptedResp accepted  = 3;  // { head_seq, stored }
  LedgersResp  ledgers   = 4;  // { repeated LedgerSummary, more }
  ForksResp    forks     = 5;  // { repeated ForkRecord }
  NotFoundResp not_found = 6;
  RejectedResp rejected  = 7;  // { RejectCode code; string message }
}}
```

`LedgerSummary` is `{ ledger, kind, head_seq, head_event, event_count,
first_seen_ms, updated_ms, has_forks }`; `ForkRecord` is `{ ledger, seq,
kept_event_id, SignedEvent conflicting, observed_ms, source_endpoint }`;
`RejectCode` covers `MALFORMED`, `TOO_LARGE`, `INVALID`, `FORK`, `UNSUPPORTED`
and `BUSY`.

Caps, enforced before any allocation sized by peer input (pitfall 7): request
frame 1 MiB, response frame 4 MiB, single event 64 KiB, `Push` at most 512
events, `Get.limit` clamped to 512, `List.limit` to 256, 64 requests per
connection, 10 MiB and 100000 events per stored ledger, 10000 ledgers per
witness.

A witness serves all five requests. Decision: a wallet serves `Head`, `Get` and
`List` for its own identities and answers `Push` with `UNSUPPORTED`, so wallets
never replicate others' ledgers and are never fork observers.

On `Push` a witness rejects oversize or malformed input before decoding, splices
the events onto any stored prefix, verifies the combined chain from nothing,
then appends atomically and answers `Accepted`. Decision: full re-verification
on every push rather than incremental trust in the stored prefix, since ledgers
are small and this keeps one verification path. If a pushed event's seq exists
locally with a different event id the witness does not overwrite: first seen
wins, it writes a `ForkRecord`, stops there and answers `Rejected { FORK }`.

Flag W stance: witnesses stay passive and unsigned, with no receipts and no
thresholds (decisions/005-witnesses). Over accepting the gap, mabel adds
duplicate-sequence rejection inside a supplied log, refusal to overwrite the
first event seen at a sequence, and fork records served through `Forks` and
shown in the debug UI. A fork record is self-authenticating although the witness
signs nothing: two validly signed events at one sequence prove the owner
equivocated. Documented as still missing: nothing forces a witness to report a
fork, and asking one witness yields one witness's view.

### 6. Stances on the digest flags

**W, equivocation.** As in section 5: no receipts, but duplicate-sequence
rejection, first-seen-wins and exposed self-verifying fork records. The residual
gap is documented, not silently dropped.

**A, anchoring versus inlining.** Payloads are inlined; there is no separate
record with its own digest and no seal anchoring, which is acceptable because
ledgers are public. Lost: proving one attestation means handing over the ledger
prefix, so the verifier sees the issuer's trust graph to that point; there is no
portable single-record file; and payload bytes are no longer stable
independently of ledger framing, so an attestation cannot be re-anchored
elsewhere. Partial disclosure is a non-goal (section 1).

**P, exact-payload binding.** The mechanism survives the policy. A controller's
signature covers the exact encoded bytes of the org event that lands in the
ledger, not a digest of a proposal, because `SignedEvent` carries those bytes
verbatim; the invitee's signature covers the exact acceptance bytes the event
embeds.

**L, liveness proof of the subject.** Out of scope, no challenge-response
(decisions/008). The issuer is responsible for confirming out of band that the
subject controls the identity. The README says so, and verifier output includes
`subject control was not proven to this verifier; the issuer is responsible for
out-of-band confirmation`.

**R, revocation completeness.** A verifier reports what it checked and where it
came from, never global completeness: `valid as of seq N of <ledger id>, fetched
from <EndpointId> at <time>; no revocation up to seq N`. It never says
"unrevoked", and every `--json` result carries `source`, `head_seq`,
`head_event` and `fetched_at`.

**D, discovery.** No global discovery and no "who trusts B" query, since
attestations live only in the issuer's ledger. The witness debug UI and `List`
enumerate what one witness holds, a diagnostic rather than an index. Documented
as a hole.

### 7. Repository and crate layout

Cargo workspace, MSRV 1.91, edition 2024 (iroh's floor).

```
proto/mabel/v0/{ledger,sync}.proto  normative schemas
crates/mabel-proto/  prost-generated types only, build.rs, no logic
crates/mabel-core/   ledger semantics, digests, byte authority, verification,
                     state fold. No tokio, no iroh, no filesystem
crates/mabel-net/    client and ProtocolHandler server, size caps, iroh, tokio
crates/mabel-node/   node home, storage, key files and permissions, wallet and
                     witness runtimes, sync orchestration, axum API, UI serving
crates/mabel-cli/    the `mabel` binary, clap, output rendering, exit codes
ui/                  vite workspace: ui/shared, ui/wallet, ui/witness
tests/e2e/           Playwright specs driving CLI plus both UIs
test-vectors/        checked-in golden bytes and digests
docker/              Dockerfile and the compose demo topology
```

Decision: a separate `mabel-proto` so exactly one crate runs `protoc` at build
time and core and net share the generated types. Decision: storage lives in
`mabel-node`, being a few hundred lines of file IO with no other consumer.
Decision: core stays async-free and IO-free, so wasm and mobile remain reachable
(decisions/001-scope) and verification cannot read local state (pitfall 5).

### 8. Storage

One home per node, `$MABEL_HOME` or `~/.mabel`, overridable with `--home`.

```
node.json                       role, http bind, default witnesses
node.key                        0600, iroh endpoint secret key
identities/<id>/meta.json       alias, kind, created_at (never signed)
identities/<id>/{active,reserve}.key   0600
ledgers/<id>/000000000000.ev    encoded SignedEvent, one file per event
ledgers/<id>/head.json          cache: seq, event id, updated_ms (rebuildable)
ledgers/<id>/meta.json          provenance: source endpoint, first seen
forks/<id>/<seq>-<event_id>.ev  conflicting events a witness rejected
peers.json                      cached ledger id to EndpointId hints, untrusted
```

Decision: plain files, no database. Event files are named by zero-padded
sequence so directory order is chain order, and the only access patterns are
"read all" and "read from seq N", both served by a sorted listing. One encoded
`SignedEvent` per file also keeps byte authority honest: the file is the signed
object and is served to peers unmodified. Writes are atomic (temp file in the
same directory, fsync, rename), and a multi-event append renames `head.json`
last so a crash leaves a shorter but valid ledger. Permissions come verbatim
from hearsay: directories 0700, key files 0600, and loading a group- or
world-readable key file fails with exit code 60 unless
`--allow-insecure-permissions` is passed.

### 9. CLI surface

Global flags: `mabel [--home PATH] [--json] [--verbose]
[--allow-insecure-permissions] <command>`. Every command supports `--json` with
a stable document, and JSON errors are `{ok, code, message, details}`. Aliases
are local and never signed; ids are authoritative.

```
identity create --alias <a> | list | show <alias|id> | export <alias|id>
trust add    --issuer <alias|id> --subject <id>
trust revoke --issuer <alias|id> --attestation <event-id>
trust list   --issuer <alias|id>
org create --alias <a> --founder <alias|id>
org show   <alias|id>
org invite --org <alias|id> --by <alias|id> --invitee <id>
           --invitee-key <key> --role member|controller --out <file>
org accept <invite-file> --as <alias|id> --out <file>
org admit  --org <alias|id> --by <alias|id> <acceptance-file>
org remove --org <alias|id> --by <alias|id> --member <id>
witness add --identity <alias|id> --endpoint <endpoint-id>
sync push  --identity <alias|id> [--to <endpoint-id>]
sync fetch <ledger-id> --from <endpoint-id>
verify ledger <ledger-id> [--from <endpoint-id>]
verify trust  --issuer <id> --subject <id> [--from <endpoint-id>]
witness run [--http <addr>] [--iroh-port <n>] | wallet serve [--http <addr>]
node id
```

Decision: `org invite` / `org accept` / `org admit` replaces hearsay's
invite/accept/finalize. Two parties sign, so three steps remain, but "admit"
names what the third step does (a controller appends the acceptance to the org
ledger) rather than a state machine that no longer exists.

Exit codes, hearsay's table minus code 40 (pending approvals, dropped with
proposals): 0 success, 2 usage, 10 invalid schema or malformed input, 20
cryptographic or semantic verification failure, 30 peer or network unavailable,
50 stale state or conflicting event or replay, 60 insecure key file permissions,
70 unsupported feature. Errors name their layer as hearsay's do: `Schema
error:`, `Ledger error:`, `Policy error:`, `State error:`, `Replay error:`,
`Network error:`.

### 10. Web UIs

The node serves an axum HTTP JSON API on loopback plus static UI assets. All
logic lives in the node; the UI calls the API, holds no keys and does no crypto.

Decision: **React 19 + Vite + TypeScript + Tailwind + shadcn/ui**, built to
static assets and embedded with `rust-embed`, with `--ui-dir` to serve from disk
in development. shadcn gives a clean look with no design work, Playwright drives
it through stable `data-testid` attributes, and the JSON API must exist anyway.
One Vite workspace, three packages: `ui/shared` (components, API client,
formatting of ids and reports), `ui/wallet` and `ui/witness`, built as two
bundles so a witness image ships no wallet code.

Wallet API under `/api`: `GET /node`; `GET|POST /identities`; `GET
/identities/:id` and `/identities/:id/ledger?since=`; `POST
/identities/:id/witnesses`; `POST /trust` and `/trust/:event_id/revoke`; `POST
/orgs`, `/orgs/:id/invites`, `/orgs/:id/acceptances`, `/orgs/:id/removals`;
`POST /sync/push`; `POST /verify`. Witness API, read-only: `GET /node`,
`/ledgers`, `/ledgers/:id`, `/ledgers/:id/events?since=`, `/forks`.

Decision: no authentication. Both servers bind `127.0.0.1` by default and the
POC does not do production key custody (decisions/001-scope); binding elsewhere
prints a warning. UI verification results use the same "as of seq N from source
S" wording as the CLI, from the same struct.

### 11. Testing strategy

- **Core unit tests**, one negative case per verification rule: broken prev
  link, duplicate sequence, gap, wrong ledger id, backwards timestamp,
  unauthorized signer, payload wrong for the ledger kind, unknown field caught
  by the `encoded_len` gate, unrecognised `oneof` variant, acceptance replayed
  to another org, to another invite and twice to one invite, removal of the last
  controller, revocation of an unknown or already revoked attestation.
- **Golden vectors** in `test-vectors/`: per event type, the encoded body bytes
  as hex, the event id, the signature under a fixed key, and a JSON rendering
  for review. Tests assert the encoder reproduces bytes and digests exactly
  (pitfall 1) and that flipping one byte fails. These double as the conformance
  suite for future non-Rust clients.
- **Protocol tests** with two in-process endpoints, `presets::Minimal` and
  `RelayMode::Disabled`, dialling the loopback `EndpointAddr` (research 002
  section 8, level 1): every request type, oversize, truncated and garbage
  input, a push of an invalid chain, and a fork push that produces a
  `ForkRecord` while the first-seen event survives.
- **CLI integration tests** with `assert_cmd` and a temp home: every command,
  every exit code, `--json` shape stability, insecure permissions (exit 60).
- **Fresh-verifier test**: wipe the home, then `verify trust` against a witness
  with no local state and no keys. Hearsay's best acceptance test, kept.
- **Playwright e2e** over the compose topology, driving both UIs and the CLI in
  one scenario: two people create identities in two wallets, configure the
  witness, push, one founds an org and invites the other, the invitee accepts in
  their own wallet UI, the founder admits, the person and the org each attest
  trust in a third identity, a stranger verifies from an empty home, the issuer
  revokes, and the witness UI shows the ledgers and heads. A second scenario
  forces a fork and asserts the UI shows the conflict and keeps the first event.
- **Containers**: a multi-stage Dockerfile (rust build, node UI build, slim
  runtime) producing one image for both roles, plus `docker/compose.yaml` with
  two wallets and one witness, keys on volumes, `portmapper` disabled, fixed UDP
  ports published and wallet HTTP ports exposed for Playwright.

### 12. Dependencies

Rust, checked against crates.io on 2026-08-23: `iroh` 1.0.3 and `iroh-base`
1.0.3 (`default-features = false, features = ["key"]` in core), `iroh-tickets`
1.0.0 (optional, LAN demo), `prost` 0.14.4 with `prost-build` 0.14.4 and
`protoc-bin-vendored` 3.2.0 so no system `protoc` is needed, `blake3` 1.8.7,
`data-encoding` 2.11.1 for base32 ids, `thiserror` 2.0.20, `serde` 1.0.229 and
`serde_json` 1.0.151 (HTTP API, config, vectors), `tokio` 1.53.1
(`rt-multi-thread, macros, fs, signal`), `axum` 0.8.9, `tower-http` 0.7.0,
`rust-embed` 8.12.0, `clap` 4.6.6, `tracing` 0.1.44 with `tracing-subscriber`
0.3.23, `anyhow` 1.0.104 (node and cli only), `getrandom` 0.4.3. Dev: `iroh`
with `test-utils`, `tempfile` 3.27.0, `assert_cmd` 2.2.2.

`iroh-base` uses `rand 0.10` and `ed25519-dalek >=3.0.0-rc.0,<4`; mabel depends
on neither directly and generates keys as 32 bytes from `getrandom` passed to
`SecretKey::from_bytes`, sidestepping the `rand_core` version dance.

Frontend, checked against npm on 2026-08-23: `react` 19.2.8, `vite` 8.2.2,
`tailwindcss` 4.3.3, `typescript` 7.0.2, `@playwright/test` 1.62.1, shadcn/ui
vendored into `ui/shared`, Node 22 LTS in the build stage.

### 13. Milestones

1. Workspace skeleton, `.proto` schemas, `mabel-proto` generation, digests,
   golden vectors, byte-authority tests.
2. Person ledger: inception, witness config, trust attestation and revocation,
   the state fold, full verification with negative tests.
3. Org ledger: inception, invite, detached acceptance, admit, removal, the
   authorized-signer-at-position rule, replay and single-use tests.
4. Node home and storage: layout, atomic writes, permissions, head cache.
5. CLI for everything local, exit codes and `--json` shapes.
6. `mabel-net`: ALPN, sync schema, caps, client and `ProtocolHandler`,
   two-endpoint loopback tests.
7. Witness runtime: verify-before-store, first-seen merge, fork records, `List`
   and `Forks`, `witness run`, witness API and debug UI.
8. Wallet runtime: `wallet serve`, HTTP API, wallet UI, `sync push` and `fetch`,
   cross-ledger verification with remote fetch.
9. Containers: Dockerfile stages, compose topology, seeded demo script.
10. Playwright e2e including the fresh verifier and the fork case, then docs,
    error wording and gap analysis against the decision records.

## Alternatives considered

- **Postcard for event bytes**: canonical by construction and smallest, but
  effectively Rust-only, which fails the planned iOS, Android and web clients.
- **Canonical JSON with sorted keys**: readable and hearsay-shaped, but it
  carries every canonicalization trap and gives no language-neutral schema.
- **A single `Event` message with an inline signature** (decisions/007's
  sketch): needs a placeholder rule to sign a message containing its own
  signature; the `bytes body` wrapper is simpler and enforces byte authority.
- **gRPC or tonic over Iroh**: HTTP/2 framing and a transport adapter on a
  transport that already multiplexes and authenticates, for five request types.
- **iroh-blobs**: content addressing cannot express "everything after seq N",
  and a ledger has a mutable head (research 002 section 7).
- **iroh-gossip**: unordered best-effort delivery to a membership set that must
  be bootstrapped, with no answer for the main read path.
- **iroh-docs**: a replicated key-value store with its own authorship model,
  duplicating the ledger mabel must write anyway, and still 0.x.
- **askama plus htmx**: less code and no node toolchain, but the owner asked for
  a well-known framework, and the JSON API is needed regardless.
- **An embedded database (redb, sqlite)**: no index exists that a sorted
  directory listing does not already serve.
- **Reusing an identity key as the node key** for free pusher authentication:
  ties transport identity to one of a wallet's many identities and exposes a
  signing key to a listener, while the chain already authenticates content.
- **Witness receipts and a threshold**, hearsay's equivocation answer: ruled out
  by decisions/005-witnesses, with fork records as the passive substitute.

## Consequences

Easier: one verification function serves the CLI, the wallet, the witness and
the fresh-verifier test, because core does no IO. A `.proto` directory plus
golden vectors is a complete implementation contract for a non-Rust client.
Dialling by `EndpointId` makes the only address in the system a 52-character
string, so witness config is a list of strings.

Harder: byte authority is a discipline the compiler cannot fully enforce, so
every path that touches an event carries bytes rather than structs and review
must watch for accidental re-encoding. Protobuf's tolerance of unknown fields is
replaced by a deliberately narrow length check, so a future client emitting a
different but equal-length encoding is accepted. Inlining payloads means proving
one attestation discloses the issuer's whole ledger prefix.

Deferred: key rotation, active equivocation defense beyond first-seen-wins,
subject liveness proof, discovery of who trusts whom, wasm and mobile builds,
and authentication on the HTTP APIs.
