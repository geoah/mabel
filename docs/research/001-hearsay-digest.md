# Hearsay digest for mabel

Source: `/tmp/hearsay` (README.md, docs/hearsay_mvp_implementation_specification.md
v1.1, ~7000 lines of Rust in src/, 7 integration tests). Mabel drops KERI/keriox
for its own hash-chained signed ledger plus Iroh, so this digest separates what
hearsay proves from how keriox happened to prove it.

## 1. Premise and user stories

Hearsay is a local Rust CLI that proves one narrow claim end to end: a person or
an organization can make a signed, anchored, independently verifiable statement
that it personally knows another person, and a stranger with an empty state
directory can check that statement without trusting any server.

The demo has four actors. Names are local aliases and are never signed; the
identifier is authoritative.

- Theo, individual, founder and controller of the Embassy.
- Zarinah, individual, invited second controller of the Embassy.
- George, individual, subject of the `knows` statements. George runs one command
  in the whole demo (exporting his contact and answering one challenge) and
  never approves, signs, or consents to anything said about him.
- Embassy, an organization with its own identifier, distinct from Theo's and
  Zarinah's.

The graph the demo builds:

```
Theo    --controller--> Embassy
Zarinah --controller--> Embassy
Theo    --knows------>  George
Embassy --knows------>  George
```

The user stories (US-01 to US-14) are: create an identity with a pre-committed
next key; rotate to that committed key without changing the identifier; exchange
contacts and validate the other side's key history; prove current control with a
fresh challenge; issue `knows`; create an organization as a separate identity;
invite a second controller; accept the invitation explicitly; finalize the
addition without changing the organization identifier; propose an organization
statement; approve it as each controller; finalize only when every approval
exists; verify all of it from an empty directory; revoke as the issuer.

Two invariants carry most of the product weight. Being named does not make you a
controller: Zarinah's signed acceptance is required even though the underlying
KERI threshold would let Theo act alone. And an organization statement is not
implied by a controller's personal statement: Theo knowing George does not make
George known to the Embassy.

`demo/complete-demo.sh` runs phases A to J: start infrastructure, create three
identities, create the Embassy, invite and add Zarinah, prove George's control,
Theo attests, the Embassy attests (including a deliberate one-approval attempt
that must exit 40 and publish nothing), verify everything from a wiped verifier
home, rotate Theo and re-verify the old attestation, then revoke the Embassy
attestation with both approvals.

## 2. Domain model

### Identifiers

An AID is a self-addressing prefix derived from the inception event.
Transferable identifiers commit at inception to a digest of the next public key
(pre-rotation). Rotation reveals exactly the committed key and commits a new one.
The identifier never changes across rotations, which is what lets an old
attestation stay valid afterwards.

Organizations are KERI group identifiers with the same shape. Hearsay hit a
collision here: a one-participant group with the same keys, thresholds and
witnesses as its founder serializes to the founder's exact inception event and
derives the same prefix. The workaround in
`src/keriox.rs:incept_single_controller_group` reverses the witness ordering so
the group inception differs.

### KEL, establishment vs interaction events

The key event log is the per-identifier append-only chain. Establishment events
(inception, rotation) change the key state. Interaction events do not change keys
and exist to anchor data: they carry a digest seal over an application record's
SAID. Hearsay never puts application payloads in the KEL; it puts only the
digest, and ships the record bytes alongside. Verification finds the interaction
event whose seal matches the record SAID
(`src/keriox.rs:find_anchor_from`).

### SAIDs and canonical serialization

Every hearsay application record is a Rust struct with an embedded `said` field.
The `said` crate (0.4.3) computes a Blake3-256 digest over the JSON
serialization with the `said` field replaced by a same-length placeholder, then
writes the digest back into the field. Consequences that matter:

- canonical bytes are `serde_json::to_vec` of the struct, so field order is
  struct declaration order, not sorted keys, and not JCS;
- every struct uses `#[serde(deny_unknown_fields)]`, so an extra field is a parse
  error, not an ignored field;
- optional fields use `skip_serializing_if = "Option::is_none"`, so their
  presence changes the bytes and therefore the digest;
- validation always recomputes the digest and rejects a mismatch
  (`DomainError::Said`), so any mutation of a record invalidates it;
- checked-in golden vectors in `test-vectors/records/*.json` plus
  `test-vectors/expected-saids.json` pin the exact bytes and digests, and a unit
  test asserts `canonical_json() == file bytes`.

Every record also carries a 32-byte random `nonce`, base64url without padding,
validated by re-encoding and comparing (so non-canonical base64 is rejected).
The nonce exists so two structurally identical records (Theo knows George, twice)
get different SAIDs.

### Canonical record types

- `KnowsV1` (`hearsay.knows/v1`): schema, said, issuer, subject, nonce. The only
  attestation type. Issuer may be a person or an organization, subject must be a
  transferable individual identifier, issuer and subject must differ. No notes,
  scores, dates, expiry, or confidence. Its SAID is anchored by the issuer.
- `RevocationV1`: schema, said, issuer, target (the `KnowsV1` SAID), nonce.
  Issuer must equal the target's issuer. Anchored by that issuer. Revocation is a
  lifecycle record, not a relationship, and never deletes anything.
- `ControllerInvitationV1`: organization, inviter, invitee,
  current_controller_set SAID, proposed_controllers (ordering is authoritative
  and doubles as the group key index map), approval_threshold, nonce. Signed by
  the inviter's personal key.
- `ControllerAcceptanceV1`: invitation SAID, organization, controller,
  controller_key_state (sequence plus establishment event digest), nonce. Signed
  by the invitee. Binding the invitee's exact key state is what makes the
  acceptance non-transplantable. Single use, tracked by a marker file
  (`Home::acceptance_was_used`).
- `ControllerSetV1`: organization, version, previous SAID, controllers,
  approval_threshold, group_key_map, effective_group_event (sequence and digest),
  invitation SAID, acceptance SAID, nonce. Version 1 is founder-only with
  threshold 1; version 2 is two controllers with threshold 2 and must carry the
  transition evidence. Only versions 1 and 2 exist.
- `OrganizationActionProposalV1`: organization, controller_set SAID, action
  (`issue-personally-known` or `revoke-personally-known` plus the record SAID),
  candidate_event_digest (digest of the exact KERI event bytes that will be
  published), proposer, nonce.
- `ControllerApprovalV1`: proposal SAID, organization, controller_set SAID,
  controller, decision, nonce. One per controller per proposal.
- `ChallengeV1` and `ChallengeResponseV1` in `src/challenge.rs`: 32-byte nonce,
  challenger, expected responder; the response repeats the challenge SAID and
  nonce and is signed under the responder's current establishment key.

Signed envelopes (`GovernanceEnvelope`, `SignedEnvelope`) wrap a record: signer
identifier, the establishment event seal that names the exact key state used, the
canonical payload as a string, and the signature. Verification re-serializes the
record and requires byte equality with the envelope payload before checking the
signature.

### Contact bundles and proof bundles

Bundles are ZIP files with the `.hsy` extension and a fixed internal layout:
`manifest.json`, `records/<said>.json`, `envelopes/<said>.cesr`,
`keri/events/<digest>.cesr`, `contacts/<aid>.json`, `receipts/...`. The manifest
is an index with a SHA-256 per file and is explicitly not a trust anchor; every
object inside is separately content-addressed or signed. Kinds: contact,
challenge, challenge-response, controller-invitation, controller-acceptance,
organization-action-proposal, controller-approval, individual-attestation-proof,
organization-attestation-proof, revocation-proof.

The reader (`src/bundle.rs`) rejects absolute paths, `..`, backslashes, directory
entries, duplicate paths, files not listed in the manifest, hash mismatches,
nested ZIPs (magic-byte check), more than 128 files, and more than 16 MiB
uncompressed. Writing is deterministic (stored entries, fixed timestamp, sorted
paths), so the same input produces byte-identical archives.

A contact bundle carries only the identifier, its witness and watcher endpoints,
and the public KEL. One demo shortcut not worth copying: `ContactRecord`
validation requires the contact's endpoints to equal the importer's own
configured network exactly, so hearsay can only import contacts from its own
witness set.

### Verification from a cold start

`hearsay verify attestation <proof>` runs from an empty home with no private
keys. For an individual proof (`src/attestation.rs:verify`): check every file
hash; parse `KnowsV1` strictly; recompute the SAID and compare with the manifest
root; validate both contact records; create a throwaway keriox store in a temp
directory; import the issuer and subject KELs from the bundle, rejecting
non-notice messages, wrong-prefix events, and duplicate sequence numbers; find
the issuer interaction event whose digest seal equals the record SAID; require
the witness receipt tally; report valid, with lifecycle status "revocation status
not established by supplied evidence" unless a revocation proof was supplied.

The organization path (`src/proposal.rs:verify_attestation`) adds: validate the
referenced `ControllerSetV1` and its transition chain, re-derive the candidate
event digest from the exact candidate bytes and require it to match the proposal,
require the candidate to anchor the record SAID under the organization prefix,
validate every approval envelope, deduplicate approvals by controller (a
duplicate is an error, not a silent drop), require every approver to be in the
referenced controller set, and require the count to reach the threshold. Removing
one approval from a finished proof makes verification fail; the tests assert it.

## 3. CLI surface

Global: `hearsay [--home PATH] [--config PATH] [--json] [--verbose]
[--allow-insecure-permissions] <command>`. Home defaults to `$HEARSAY_HOME` or
`~/.hearsay`; config defaults to `<home>/config.json`. Every command supports
`--json` with a stable document.

```
identity create --alias <a>
identity show --alias <a>
identity rotate --alias <a>
identity verify <AID>
identity export-contact --alias <a> --out <f>
identity import-contact <f>

challenge create <RESPONDER_AID> --out <f>
challenge respond <challenge> --alias <a> --out <f>
challenge verify <challenge> <response>

attest knows <SUBJECT_AID> --issuer <a> --out <f>
attest revoke <target-proof> --issuer <a> --out <f>

org create <alias> --controller <a>
org show <alias>
org controller invite <alias> --controller <AID> --out <f>
org controller accept <invitation> --alias <a> --out <f>
org controller finalize <alias> <invitation> <acceptance> --out <f>
org attest propose <alias> knows <SUBJECT_AID> --proposer <a> --out <f>
org attest revoke-propose <alias> <target-proof> --proposer <a> --out <f>

proposal approve <proposal> --controller <a> --out <f>
proposal finalize <proposal> <approval>... --finalizer <a> --out <f>

verify attestation <proof>
verify controller-set <proof>
verify revocation <target-proof> <revocation-proof>
```

Exit codes, asserted by the tests and a good template: 0 success, 2 usage, 10
invalid schema or malformed bundle, 20 cryptographic or semantic verification
failure, 30 infrastructure unavailable or insufficient receipts, 40 valid but
pending approvals, 50 stale state or conflicting event or replay, 60 insecure key
file permissions, 70 unsupported feature. Errors name their layer ("Schema
error:", "KERI error:", "Policy error:", "State error:", "Replay error:") rather
than collapsing to "verification failed"; `--json` errors carry
`{ok, code, message, details}`.

Local state per home: `config.json`, `keri/<alias>/{db,priv_key,next_priv_key,
id,identity.json}`, `contacts/`, `records/<said>.json`, `envelopes/`,
`organizations/`, `proposals/`, `proofs/`. Directories are 0700, key files 0600,
and loading refuses group- or world-readable key files unless
`--allow-insecure-permissions` is passed (exit 60). Identity creation writes to a
temp directory and renames into place; rotation writes a journal first and
completes on rerun if it crashed mid-way.

## 4. Hearsay to mabel mapping

| Hearsay concept | Mabel |
|---|---|
| KERI AID (self-addressing prefix from inception) | Keep the shape: identity id is the digest of the inception event of its own ledger. Own derivation, no CESR. |
| KEL | Keep as mabel's hash-chained signed ledger, one per identity. |
| Establishment vs interaction events | Simplify. Inception is the only establishment event in scope; everything else (witness config, trust, membership) is an ordinary chained event. Keep the distinction in the type system so rotation can be added later. |
| Pre-rotation (next-key digest commitment) | Keep the commitment in inception (the reserve key). Drop rotation itself. |
| Rotation, rotation journal, atomic seed promotion | Drop for the POC. Keep the journal idea in mind if rotation lands. |
| Witnesses as receipt-producing keriox services | Simplify to passive Iroh replicas that store and serve ledgers. |
| Witness receipts, 2-of-3 threshold, mailbox polling | Drop. See flag W below. |
| Watcher, OOBI resolution | Drop. Iroh NodeIds in the witness config event replace endpoint discovery. |
| `config.json` network file with fixed 3 witnesses | Replace with the on-ledger witness config event. |
| SAID over canonical JSON, Blake3-256 | Keep the idea, own implementation. See flag C. |
| `deny_unknown_fields`, recompute-and-compare on every parse | Keep verbatim. Cheap and load-bearing. |
| 32-byte nonce per record | Probably drop: sequence number plus previous-event hash already make each event unique. Keep only for off-ledger challenge-like payloads. |
| Golden test vectors pinning bytes and digests | Keep. |
| `KnowsV1` | Becomes the trust attestation event in A's ledger, one-way, no subject participation. |
| `RevocationV1` | Becomes the trust revocation event in A's ledger, referencing the attestation event id. |
| Interaction event anchoring a record SAID | Collapse: the event is the record. See flag A. |
| Organization as a separate group identifier | Keep: orgs get their own ledger and their own id. |
| `ControllerSetV1` versions 1 and 2, group_key_map | Simplify to membership events (create org, invite, acceptance, removal) with roles member and controller. No key map, since org events are signed by a controller's personal key, not by group keys. |
| `ControllerInvitationV1` + `ControllerAcceptanceV1` | Keep both, and keep the acceptance binding the invitee's exact key state. Record both in the org ledger. |
| Group rotation to add a controller key (the preparation-rotation dance) | Drop entirely. Membership is data, not key state. This is the single largest simplification. |
| `OrganizationActionProposalV1` + `ControllerApprovalV1` + threshold 2 | Drop. Any single current controller signs. See flag P. |
| Exit code 40 (pending approvals) | Drop with proposals; keep the rest of the code table. |
| Challenge/response bundles | Drop. See flag L. |
| Contact bundle (`.hsy` ZIP with KEL and endpoints) | Replace with ledger sync over Iroh. Keep the concept of a shareable identity descriptor: id plus replica NodeIds. |
| Proof bundles (per-claim ZIP with hashed manifest) | Optional. If mabel keeps any file export, keep the bundle reader's hardening verbatim. |
| Fresh-verifier-from-empty-home test | Keep. It is the best single acceptance test in the project. |
| Alias never signed, identifier authoritative | Keep verbatim. |
| Plaintext seeds, 0700/0600, refuse insecure perms, exit 60 | Keep verbatim. |
| Layered error strings and `--json` error envelope | Keep. |
| Honest status language ("not established by supplied evidence") | Revisit. See flag R. |

### Flags: things the decisions above would silently lose

**W. Equivocation and duplicity detection.** Hearsay's only defense against an
issuer publishing two different events at the same sequence number is the witness
receipt tally: witnesses apply KERI first-seen semantics, and a verifier requires
two independent receipts. Mabel's passive replicas produce no receipts and sign
nothing, so nothing stops A from serving fork X to one peer and fork Y to
another, and nothing lets a verifier detect it. Options short of receipts: have
replicas record and expose first-seen ordering, have peers gossip observed heads,
or accept the gap and document it. Do not let it disappear silently. Note that
hearsay's own KEL import does catch duplicate sequence numbers within a single
supplied log (`src/attestation.rs:import_kel`, `src/keriox.rs:merge_and_verify_kel_into`),
which mabel should keep as the cheap half of the defense.

**A. Anchoring versus inlining.** Hearsay deliberately splits the record (canonical
JSON, self-addressed, portable) from the anchor (a ledger event carrying only the
digest). That split is what lets Theo hand someone a single `knows` record plus a
slice of KEL without exposing the rest of his log, and it is what makes the record
bytes stable independently of ledger framing. If mabel inlines payloads into
events, decide explicitly what replaces that: how does a verifier check one
attestation without receiving the issuer's entire trust graph, and is partial
disclosure a goal at all?

**P. Exact-payload approval binding.** Even with single-controller signing, keep
the mechanism, not just the policy. Hearsay's approvals sign the digest of the
exact bytes to be published, so a proposal cannot be mutated between approval and
execution. With one controller the window shrinks but does not close: the org
event a controller signs must be byte-identical to the org event that lands in
the ledger, and the signature must cover the event, not a summary of it.

**L. Liveness proof of the subject.** Challenge-response is how Theo confirmed
that George actually controls the identifier before attesting about it. Dropping
it means a mabel trust attestation names an identifier its issuer may never have
seen anyone use. The demo narrative depends on this step (phase E). Consider at
minimum documenting that the issuer is responsible for out-of-band confirmation,
or keep a one-command signed-nonce check as it is cheap.

**R. Revocation completeness.** Hearsay refuses to claim an attestation is
unrevoked because it has no complete index. Mabel replicating whole ledgers can do
better, but only if a verifier can tell it holds A's ledger up to A's current
head. That needs a defined head concept and a rule for what "complete enough"
means. Otherwise mabel inherits the same hedge while looking like it does not.

**D. Discovery.** Trust attestations live only in the issuer's ledger, so "who
trusts B" requires enumerating ledgers. Hearsay had the same hole and named it.
Decide whether mabel answers it (a replica index, a query over Iroh) or names it.

## 5. Pitfalls to carry over

1. **Canonicalization is the whole game.** Field order, optional-field omission,
   integer formatting, and string escaping all move the digest. Pick one rule,
   implement it in one function used by both signing and verification, and pin it
   with checked-in golden vectors that assert exact bytes and exact digests, as
   `src/domain.rs` tests do. Never recompute a digest from a re-parsed struct
   without also comparing the bytes.

2. **Strict parsing, always.** `deny_unknown_fields` plus recompute-and-compare on
   every deserialization is what makes tampering fail closed. An unknown field
   silently ignored is a forgery vector, since the digest was computed over
   different bytes than the ones a lenient parser accepts.

3. **Signatures must cover exact bytes, and the envelope must carry the key
   state.** Hearsay's envelopes name the establishment event seal used to sign, so
   the verifier resolves the exact historical key rather than the current one.
   This is what keeps Theo's old attestation valid after his rotation. Mabel needs
   the same even with rotation out of scope: an event must be verified against the
   key that was active at that point in the chain, not against whatever key the
   ledger head shows.

4. **Replay and single-use.** An acceptance must not be reusable for another
   organization, another invitation, or a second time. Hearsay binds organization,
   invitation SAID, invitee identifier and invitee key state into the acceptance,
   and additionally keeps a used-acceptance marker file. The marker is a local
   hack; mabel gets the durable version for free by recording the acceptance in the
   org ledger and rejecting a second one, but must actually implement that check.
   Also reject stale references: hearsay's exit code 50 covers "the controller set
   moved under you".

5. **Verification from scratch is a separate code path and must be tested as
   one.** Anything that reads local state during verification is a bug that hides
   until someone else runs the check. Hearsay builds a throwaway store in a temp
   directory, imports only what the proof carries, and the demo wipes the verifier
   home before every check. Keep both the temp-store discipline and the wiped-home
   test.

6. **Identifier derivation collisions.** Two inception events with the same
   content derive the same identifier. Hearsay hit this with a one-member group
   versus its founder and worked around it by permuting witness order. Mabel must
   ensure a person inception and an org inception cannot produce identical bytes,
   ideally by an explicit type field rather than an accident of ordering.

7. **Bundle and input hardening.** If any file format survives into mabel, keep
   the checks: no absolute paths, no `..`, no duplicate entries, no unlisted
   files, per-file hashes, no nested archives, and hard caps on file count and
   uncompressed size. The same caps apply to anything arriving over Iroh: a peer
   serving a ledger is untrusted input with a size limit.

8. **Say what you verified, not that it is true.** `knows` proves who made a
   statement, not that the statement is correct, and it is not proof of legal
   identity or unique humanity. Hearsay puts this in the verifier output, the
   README and the spec. Mabel's trust attestations need the same sentence in the
   same three places.
