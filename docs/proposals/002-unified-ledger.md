# 002: one ledger type with a principal set

- Date: 2026-08-24
- Status: proposed
- Decisions affected: implements 012; extends 003 and 004 to every ledger;
  amends 002's person/organization framing; supersedes parts of proposal 001
  sections 3.1 to 3.6, 9 and 10 (listed in Migration)

## Context

Proposal 001 gives mabel two ledger types. A person ledger is self-keyed and
takes trust events; an organization ledger holds no key of its own, is
controlled by people, and is the only place membership events are legal. The
split duplicates the fold, the field table, the commands and the screens, denies
a person any way to delegate signing to a second device, and makes "agent" or
"service" cost a third ledger type.

The two shapes differ in exactly one way: where the first signing authority
comes from. The chain, the digests, the trust events and the witness config are
identical. This proposal keeps that one difference and deletes the rest.

## Proposal

### 1. One ledger, one principal set

There is one ledger type, and its folded state carries a **principal set**: each
principal is `(identity id, ed25519 public key, role)`, with `role` in `MEMBER`
or `CONTROLLER` and nothing else. `MEMBER` remains recorded data with no signing
authority, exactly as in 001 section 3.4; only `CONTROLLER` principals append.
`PersonState` and `OrgState` disappear, and the principal set plus the inception
root is the whole authority model.

### 2. Inception carries a root discriminator

Every ledger starts with one `Inception` event whose payload holds exactly one
cryptographic **root**, as a `oneof`:

- a **raw root**, `active_key` plus `reserve_commit`, today's person shape, so
  the ledger is self-keyed. Decision: that root key is a permanent `CONTROLLER`
  principal whose identity id is the ledger's own id and is **not removable in
  this POC**. Delegation is continuity, not recovery: adding principals must
  never become a way to take a ledger away from its root.
- an **identity root**, exactly **one** founding principal: identity id, key and
  that identity's inception bytes, checked by the binding rule 001 section 3.4
  already defines. Decision: one founding principal, not a set, because a
  single-signature envelope cannot carry consent from several founders and
  consent is the point of decision 004; co-founders join by invitation.

The root is the only thing separating what 001 called a person ledger from an
organization ledger, and it is a cryptographic fact, not a label.

### 3. Declared kind is advisory

`kind` survives as an advisory enum with `KIND_UNSPECIFIED` (rejected),
`PERSON`, `ORGANIZATION`, `AGENT`, `SERVICE`. Decision: it never gates payload
validity, authorization, or any verification outcome. Verifier and UI output
call it the **declared kind** so nobody reads it as a checked claim.

Restating hearsay pitfall 6 for the unified inception: two inceptions with
identical bytes derive identical ids, and 001 defended against that with
different payload tags for person and organization. That defense is now the root
`oneof`, whose tags differ; two identity roots differ by founder and two raw
roots by `active_key`. The 16-byte `nonce` rule is unchanged and still covers
one founder creating two ledgers in one millisecond. The advisory kind carries
no part of this.

### 4. Membership on any ledger

`MembershipInvitation`, `MembershipAcceptance` and `MembershipRemoval` are legal
on **any** ledger, so a raw-rooted ledger gets delegation for free: a person
invites their laptop or a co-signer as a `CONTROLLER`, and the invitee's signed
acceptance is required exactly as decision 004 demands. The detached acceptance
blob keeps every binding field; `org` becomes `ledger`. Constraints, all checked
against the state before the event:

- reject an invitation whose `invitee` equals the ledger id, so the root
  principal cannot be shadowed by an ordinary one;
- reject a principal whose key duplicates the key of an existing principal, so
  one key is never two principals and removal counts stay honest;
- a removal must leave at least one `CONTROLLER`, counted over **distinct
  keys**; the raw root is never removable and does not depend on that count.

Acceptance single use and its branch-local caveat carry over from 001 section
3.5 verbatim: no earlier acceptance on this branch may reference the same
invitation event, and the same acceptance can still appear on two divergent
branches, which fork detection surfaces rather than prevents (flag W).

### 5. Authorization and verification output

The authorization rule is uniform: `author_key` must equal the key of a
`CONTROLLER` principal in the state folded from `0..=i-1`. Seq 0 self-authorizes
under its root, the `active_key` for a raw root and the founding principal's key
for an identity root.

Trust semantics are unchanged (decision 003). Decision: verification output
names the **signing principal** on every event and every trust answer, the
`author_key` and the principal it matched, so a delegate's signature is never
silently attributed to the ledger's subject. `verify trust` keeps its pinned
answer from 001 section 9 and gains the attestation's signing principal.

### 6. Naming

Decision: one scheme across CLI, HTTP and UI, under decision 012.
`organization` is a declared kind, not a ledger type, so naming the principal
commands after it would put the split back in the interface: nobody should type
`mabel organization invite --organization <their own person ledger>`. The
neutral, accurate word is **membership**.

- CLI: `mabel identity create --alias <a> [--kind person|organization|agent|
  service] [--founder <alias|id>]`, where `--founder` selects an identity root
  and its absence a raw root; `mabel membership invite|accept|admit|remove
  --ledger <alias|id>`. Hidden undocumented aliases: `org`, `member`.
- HTTP: `POST /identities {alias, kind, founder?}`; `POST
  /identities/:id/memberships/invitations`, `/acceptances`, `/removals`;
  `GET /identities/:id` returns `principals` and `declared_kind`. The `/orgs`
  routes of 001 section 10 are deleted.
- UI: one identity screen with a Principals panel for every ledger the wallet
  controls.

### 7. Schema, rewritten in place

```protobuf
enum DeclaredKind { KIND_UNSPECIFIED = 0; PERSON = 1; ORGANIZATION = 2;
                    AGENT = 3; SERVICE = 4; }
enum Role { ROLE_UNSPECIFIED = 0; MEMBER = 1; CONTROLLER = 2; }
message RawRoot      { bytes active_key = 1; bytes reserve_commit = 2; }
message IdentityRoot { bytes founder = 1; bytes founder_key = 2;
                       bytes founder_inception = 3; }
message Inception {
  DeclaredKind kind = 1;   // advisory, must not be KIND_UNSPECIFIED
  bytes nonce = 2;         // 16 bytes
  oneof root { RawRoot raw_root = 10; IdentityRoot identity_root = 11; }
}
message MembershipInvitation { bytes invitee = 1; bytes invitee_key = 2;
                               Role role = 3; bytes invitee_inception = 4; }
message MembershipAcceptance { bytes acceptance = 1; bytes signature = 2; }
message MembershipRemoval    { bytes target = 1; }
message Acceptance { uint32 version = 1; bytes ledger = 2;
                     bytes invitation_event = 3; bytes invitee = 4;
                     bytes invitee_key = 5; }
// EventBody.payload tags: inception 10, witness_config 11,
// trust_attestation 12, trust_revocation 13, membership_invitation 14,
// membership_acceptance 15, membership_removal 16; reserved 20 to 29.
```

Decision: this rewrites proto `v0` **in place**, an explicit one-time exception
to the append-only versioning rule of 001 section 3.1. Nothing is deployed, no
ledger exists outside tests, and ledger ids commit to inception bytes, so no
in-place conversion is possible later: the exception is available once and
expires with the first ledger created outside the test suite. Signature and
digest domains are unchanged and every `Org*` name disappears (decision 012).
Decision: `sig` becomes `signature` everywhere, including `SignedEvent`, since
the rewrite already touches every vector and `sig` is an abbreviation decision
012 forbids.

`files.proto`: `InviteBundle` becomes `InvitationBundle` and its `org_prefix`
field becomes `ledger_prefix`.

### 8. Field table for the changed messages

Rows for `SignedEvent`, `EventBody`, `WitnessConfig`, `TrustAttestation` and
`TrustRevocation` carry over from 001 section 3.4 unchanged, with `sig` read as
`signature`. These rows replace the inception and `Org*` rows.

| Field | Presence | Bytes | Rule |
|---|---|---|---|
| `Inception.kind` / `.nonce` | required | - / 16 | a defined kind, never `KIND_UNSPECIFIED`; random nonce |
| `Inception.root` | required | - | exactly one variant, recognised |
| `RawRoot.active_key`, `.reserve_commit` | required | 32 | must differ |
| `IdentityRoot.founder`, `.founder_key`, `.founder_inception` | required | 32, 32, <= 1024 | id and key match the embedded standalone-valid raw-rooted inception |
| `MembershipInvitation.invitee`, `.invitee_key`, `.invitee_inception` | required | 32, 32, <= 1024 | as for `IdentityRoot`; `invitee` differs from the ledger id |
| `MembershipInvitation.role` | required | - | `MEMBER` or `CONTROLLER` |
| `MembershipAcceptance.acceptance` / `.signature` | required | <= 1024 / 64 | canonical `Acceptance`; invitee signature over `accept_input` |
| `MembershipRemoval.target` | required | 32 | a current principal or open invitee, not the raw root |
| `Acceptance.*` | `version` absent, rest required | 32 each | `ledger` equals this ledger id; `invitation_event` names an open invitation |

### 9. Deferred, with the slots named

- **Identity principals nested deeper than one level** (an organization
  controlling an organization): the embedded inception check requires a raw
  root, capping depth at 1, and lifting it means recursive verification and a
  cycle rule. Slot: the root `oneof` and `reserved 20 to 29` in the payload tags.
- **Signing thresholds**, k-of-n controllers. Slot: a `threshold` field on a
  future governance payload; the principal set already counts distinct keys.
- **Narrower capabilities** than `CONTROLLER`. Slot: `Role` values 3 and up.
- **Key rotation**, still out of scope (decision 008); `reserve_commit` remains
  the only artifact.

### 10. Migration

Nothing shipped. Tickets 002, 003 and 007 are merged; `crates/mabel-core/src/
fold.rs` is in flight under tickets 004 and 005 and should absorb this before it
lands. Surviving untouched: the four digest domains and `event_id` derivation
(`digest.rs`), the canonical encoder (`encoding.rs`), the scanner machinery in
`validate.rs` (`MessageDescriptor`, `FieldDescriptor`, `Oneof`, `Scanned`,
`scan`, `MAX_NESTING`), the id codecs (`id.rs`), the on-disk layout and atomic
writes (`mabel-node`), and every transport frame, cap and budget (`sync.proto`,
001 section 5). Changing:

- `proto/mabel/v0/ledger.proto`: section 7, including the payload tag
  renumbering; `files.proto`: the two renames.
- `crates/mabel-core/src/sign.rs`: `build_person_inception` and
  `build_org_inception` collapse into `build_inception` taking a root;
  `build_org_invite|acceptance|removal` become `build_membership_*`;
  `build_acceptance` keeps its name with renamed fields.
- `crates/mabel-core/src/validate.rs`: `PERSON_INCEPTION` and `ORG_INCEPTION`
  become `INCEPTION`, `RAW_ROOT`, `IDENTITY_ROOT`; the `ORG_*` descriptors
  become `MEMBERSHIP_*`; the `EVENT_BODY` oneof is renumbered; "kind matches the
  variant" becomes "kind is defined" plus "exactly one root variant"; and
  `verify_inception_standalone` returns the root key and requires a raw root.
- `crates/mabel-core/src/fold.rs`: `LedgerKind` becomes the advisory declared
  kind; `PersonState` and `OrgState` collapse into the principal map plus the
  root; seeding takes the root; `authorized_signer` checks `CONTROLLER`
  principals; removal counts distinct keys and refuses the raw root. Ticket 004
  was briefed to keep seeding abstract, so this is the seam it left.
- `test-vectors/`: all nine regenerate. `05-org-inception` becomes
  `05-identity-root-inception`, `06` to `08` become the membership vectors, and
  a new vector covers a raw-rooted ledger adding a second controller. Rejection
  vectors gain: invitee equal to the ledger id, duplicate principal key, removal
  of the raw root, removal leaving no controller, unset kind, no root variant.
- `crates/mabel-node/src/home.rs`: `IdentityKind` gains `Organization` (renamed
  from `Org`), `Agent` and `Service`, and becomes advisory. Key custody changes
  shape: a ledger may hold no key of its own yet be controllable locally, so
  `IdentityMeta` records which local identity signs for it and
  `identity_active_key` resolves through that link instead of erroring.
- Tickets: 001 lands the unified schema; 003 takes the descriptor and field
  table rename; 004 and 005 merge into one fold ticket retitled to membership;
  006 takes the `files.proto` renames; 008 gains `--kind` and `--founder` on
  `identity create`; 018 becomes the membership command ticket; 019's screens
  become the Principals panel; 012 takes the route rename.

Decision 013 applies to the regenerated suite: vector and fold tests stay in
milliseconds, and the added rejection cases are pure byte tests.

## Alternatives considered

- **Keeping the person and organization split.** It lost on three counts: it
  duplicates the fold, the field table, the CLI and the UI for one bit of
  difference; it denies a person any delegation, so a lost laptop is a lost
  identity with no path short of rotation; and it fixes the taxonomy in the wire
  format, so `AGENT` and `SERVICE` would each cost a ledger type. Unification
  loses its structural guarantee that membership events cannot appear on a
  person's ledger; the fold enforces the equivalent constraints at runtime.
- **Advisory kind as the discriminator**, one inception message with `kind`
  selecting the rules. Rejected: a label would carry cryptographic weight, and
  pitfall 6 is precisely about ids colliding when the distinguishing content is
  cosmetic.
- **Multiple founding principals in an identity root.** Rejected on consent: a
  single-signature envelope proves only that the signer agreed, so co-founders
  would be added unilaterally, which decision 004 forbids.
- **A removable raw root.** Rejected on seizure: a controller able to remove the
  root could take the ledger from the person it names, turning delegation into a
  recovery mechanism this POC has not designed.
- **A different verb than `membership`** (`delegate`, `principal`): `delegate`
  reads wrong for adding a `MEMBER`, and `principal` names the state, not the
  action.

## Consequences

Easier: one fold, one field table, one command set, one screen. Delegation
arrives for free on person ledgers, and `AGENT` and `SERVICE` ledgers cost a
declared-kind value and no code. The authorization rule shrinks to one sentence
covering every ledger and every event.

Harder: the ledger no longer tells a reader "this is a person" as a checked
fact, so every surface must say declared kind and mean it. Naming the signing
principal in output becomes load-bearing rather than cosmetic, because a
delegate's signature is now normal. The permanent raw root is a real
restriction: a person whose root key leaks cannot remove it in this POC and has
no recourse but a new identity until rotation lands.

Deferred: nesting beyond depth 1, thresholds, capabilities narrower than
`CONTROLLER`, and rotation, each with its slot named in section 9.
