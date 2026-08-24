# Contracts

The frozen wire contract between the node HTTP APIs, the `mabel --json`
output and the two UI routes. UI, CLI and HTTP work proceeds in parallel
against these fixtures instead of against each other.

The fixtures are normative. A response or a `--json` document that does not
match the shape here is a bug in the node or the CLI, not in the fixture.
Changing a fixture means updating every consumer in the same change: the
axum handlers in `crates/mabel-node`, the renderers in `crates/mabel-cli`,
and the types in `ui/`.

Sources: proposal 001 sections 6, 9 and 10 plus its clarifications, decision
012 (full words in names), `proto/mabel/v0/sync.proto` and
`crates/mabel-node/src/config.rs`. Where this folder decides something those
do not settle, the decision is listed under "Decisions taken here".

Proposal 003 amends the payload-table freeze below: that table was frozen
before payload tag 17 existed, and `profile_update` is now one of its rows.
Proposal 003 sections 1 to 5 are the source for the profile, verification,
contact, lookup and graph surfaces. Proposal 005 amends that row again with
`email`, and is the source for the public email and for creation with a
profile.

## Index

| File | Surface |
|---|---|
| `http/wallet-get-node.json` | `GET /api/node`, one document for every node |
| `http/wallet-get-identities.json` | `GET /api/identities` |
| `http/wallet-post-identities.json` | `POST /api/identities` |
| `http/wallet-get-known-identities.json` | `GET /api/identities/known?offset&limit` |
| `http/wallet-get-identity.json` | `GET /api/identities/:identity_id` |
| `http/wallet-get-identity-ledger.json` | `GET /api/identities/:identity_id/ledger?since=` |
| `http/wallet-get-identity-keys.json` | `GET /api/identities/:identity_id/keys` |
| `http/wallet-post-identity-profile.json` | `POST /api/identities/:identity_id/profile` |
| `http/wallet-post-identity-verification.json` | `POST /api/identities/:identity_id/verification` |
| `http/wallet-get-identity-contact.json` | `GET /api/identities/:identity_id/contact` |
| `http/wallet-put-identity-contact.json` | `PUT /api/identities/:identity_id/contact` |
| `http/wallet-post-identity-fetch.json` | `POST /api/identities/:identity_id/fetch` |
| `http/wallet-get-lookup.json` | `GET /api/lookup/:identity_id?from=` |
| `http/wallet-get-resolve.json` | `GET /api/resolve?input=` |
| `http/wallet-get-witnesses.json` | `GET /api/witnesses` |
| `http/wallet-get-witness-holdings.json` | `GET /api/witnesses/:identity_id/holdings?offset&limit` |
| `http/wallet-get-graph.json` | `GET /api/graph` |
| `http/wallet-post-graph-sync.json` | `POST /api/graph/sync` |
| `http/wallet-post-identity-witnesses.json` | `POST /api/identities/:identity_id/witnesses` |
| `http/wallet-get-identity-memberships.json` | `GET /api/identities/:identity_id/memberships` |
| `http/wallet-post-membership-invitations.json` | `POST /api/identities/:identity_id/memberships/invitations` |
| `http/wallet-post-membership-acceptances.json` | `POST /api/identities/:identity_id/memberships/acceptances` |
| `http/wallet-post-membership-admissions.json` | `POST /api/identities/:identity_id/memberships/admissions` |
| `http/wallet-post-membership-removals.json` | `POST /api/identities/:identity_id/memberships/removals` |
| `http/wallet-post-trust.json` | `POST /api/trust` |
| `http/wallet-post-trust-revoke.json` | `POST /api/trust/:event_id/revoke` |
| `http/wallet-post-sync-push.json` | `POST /api/sync/push` |
| `http/node-get-forks.json` | `GET /api/forks?ledger_id&offset&limit` |
| `cli/identity-create.json` | `mabel identity create --json` |
| `cli/identity-list.json` | `mabel identity list --json` |
| `cli/identity-show.json` | `mabel identity show --json` |
| `cli/identity-export.json` | `mabel identity export --json` |
| `cli/identity-share.json` | `mabel identity share --json` |
| `cli/profile-replace.json` | `mabel profile replace --json` |
| `cli/contact-set.json` | `mabel contact set --json` and `mabel contact show --json` |
| `cli/graph-sync.json` | `mabel graph sync --json` and `mabel graph status --json` |
| `cli/lookup.json` | `mabel lookup <identity_id> --from --json` |
| `cli/trust-add.json` | `mabel trust add --json` |
| `cli/trust-revoke.json` | `mabel trust revoke --json` |
| `cli/trust-list.json` | `mabel trust list --json` |
| `cli/witness-add.json` | `mabel witness add --json` |
| `cli/witness-run.json` | `mabel witness run --json` |
| `cli/witness-set-default.json` | `mabel witness set-default --json` |
| `cli/membership-invite.json` | `mabel membership invite --json` |
| `cli/membership-accept.json` | `mabel membership accept --json` |
| `cli/membership-admit.json` | `mabel membership admit --json` |
| `cli/membership-remove.json` | `mabel membership remove --json` |
| `cli/membership-list.json` | `mabel membership list --json` |
| `cli/sync-push.json` | `mabel sync push --json` |
| `cli/sync-fetch.json` | `mabel sync fetch --json` |
| `cli/node-id.json` | `mabel node id --json` |
| `cli/node-ticket.json` | `mabel node ticket --json` |
| `cli/wallet-serve.json` | `mabel wallet serve --json` |
| `cli/verify-trust.json` | `mabel verify trust --json` |
| `cli/verify-ledger.json` | `mabel verify ledger --json` |
| `cli/dev-seed.json` | `mabel dev seed --json` |
| `cli/errors.json` | the error envelope, one case per exit code and layer prefix |

`mabel identity rotate` has no fixture: it exits 70 with the error envelope
`cli/errors.json` already pins. `wallet serve` and `witness run` print their
document when the process stops, so their one case is the shutdown document.

`cli/dev-seed.json` carries the identity document of `cli/identity-list.json`
inside its `identities` array, and the `Pushed` and `GraphStatus` documents of
`cli/sync-push.json` and `cli/graph-sync.json` inside `pushed` and `graph`. A
seeded home is an ordinary home, so the answer to "what did the seed create" is
the same document every other surface reports. Its `identities` are in creation
order, alice then bob then carol then acme, not by ascending id.

Each `http/*.json` holds `route`, `method`, `request` (an example body, or
`null` for GET), `response` (an example 200 body) and `errors` (examples of
`{status, body}`). Each `cli/*.json` holds `cases`, one case per outcome worth
pinning, each with `case`, `command`, `exit_code` and `document`. Every file
but one names its subject in `command`; `cli/errors.json` covers no single
command, so it carries `envelope` instead.

## Conventions

**Field names.** snake_case, full words, no abbreviations: `identity_id`,
`declared_kind`, `attestation_event`, `storage_capacity`. Decision 012
applies to JSON fields and route paths, so `organization` never appears as
`org`. One thing keeps one name across both surfaces: a ledger is
`ledger_id` in the witness API and in the CLI, never `ledger` in one and
`ledger_id` in the other.

**Ids and byte fields.** Every byte field renders as lowercase RFC 4648
base32 without padding: 32-byte values (identity ids, ledger ids, event ids,
public keys, endpoint ids) are 52 characters, a 16-byte nonce is 26. Parsing
is case-insensitive. `node.json` on disk is the exception, because
`iroh_base::EndpointId` serializes as hex there. `GET
/api/identities/:identity_id/keys` renders `active_secret_key` and
`reserve_secret_key`, the 32 raw secret-key bytes, in that same base32, not in
the hex `identities/<id>/active.key` holds on disk: every key value that
crosses an API document is base32, whatever the file it came from. The two
secrets in that fixture are 32 bytes of `0x11` and 32 bytes of `0x33`; the
first is the key `test-vectors/01-raw-root-inception.json` signs Alice's
inception with, so `active_secret_key` and `active_key` are a real pair, while
the reserve secret is a stand-in because a commitment does not yield its
preimage.

**Artifacts over JSON.** The three file artifacts of proposal 001 section 3.8
(`IdentityDescriptor`, `InvitationBundle`, `AcceptanceFile`) cross the HTTP
surface as standard RFC 4648 base64 with padding, in a field whose name ends
`_base64`: `invitee_descriptor_base64`, `invitation_bundle_base64`,
`acceptance_base64`. Base32 spells ids because they are read aloud and
compared by eye; a bundle carries up to 1 MiB, where base32 would cost 60% and
base64 costs 33%. The bytes are the same bytes the file on disk holds, so
`mabel membership invite --out acme.invitation` and the HTTP route hand out the
same artifact. Byte fields *inside* an event payload stay base32 like every
other byte field. The base64 values in the fixtures are placeholders: a real
artifact depends on keys the fixtures do not hold.

**Numbers.** Sequences (`seq`, `head_seq`, `since`, `at_seq`) are JSON
numbers, 0-based, never strings. Counts (`event_count`, `stored`,
`revoked_count`) are numbers.

**Timestamps.** Unix milliseconds as numbers, named `*_ms`:
`timestamp_ms`, `created_at_ms`, `fetched_at_ms`, `observed_ms`,
`first_seen_ms`, `updated_ms`. Only the rendered `statement` sentence
carries a human time, as RFC 3339 UTC.

**Declared kind.** `declared_kind` names what an identity says it is. The
closed set is `person`, `organization`, `agent`, `service`. Proposal 001
mints only `person` and `organization`, so those are the only values in the
fixtures; a node that meets `agent` or `service` in input it cannot handle
answers code 70.

**Nullability.** A field that does not apply is present and `null`, never
absent: `prev` and `ledger_id` are `null` in a seq-0 event document,
`attestation_event` is `null` when `trusted` is false. Arrays are empty, not
null. Consumers can rely on every key in these fixtures existing.

One exception: fields the document defines as root-dependent are omitted when
the root does not carry them: `active_key`, `reserve_commit`. An
identity-rooted ledger holds no key of its own, so its identity document has
no key field to null out, and a consumer reads their absence as the root kind
(`wallet-get-identities.json`, the first entry).

**Ordering.** `GET /api/identities` and `GET /api/identities/known` sort by
ascending id, matching the `List` request in `sync.proto`, so paging is stable.
Events sort by ascending `seq`. A `List` answers the ledgers a node signs for
plus the ones it keeps as a witness, not everything it stores (proposal 006
section 8).

**Paging.** `offset`, `limit` and `more` on every paged route, echoed back in
the response. `GET /api/identities/known` defaults `limit` to 100 and clamps it
to 256, which is `MAX_LIST_LIMIT` in `mabel-net`. `?since=` is inclusive: the response starts at `seq == since`
(proposal 001, clarifications).

## The envelope

Every document, HTTP body or `--json` output, has a top-level `ok` boolean.

Success is `{"ok": true, ...}` with the payload flat at the top level.
Failure is the error envelope:

```json
{"ok": false, "code": 20, "message": "Ledger error: ...", "details": {"reason": "prev_mismatch"}}
```

`code` is the CLI exit code the same failure produces, on HTTP too, so one
table covers both surfaces. `message` is one line for a human. `details` is
an object whose `reason` is a stable snake_case class name; everything else
in `details` is specific to that reason. Consumers branch on `code` and
`details.reason`, never on `message`.

| Code | Meaning | Message prefix | Typical HTTP status |
|---|---|---|---|
| 2 | usage, unknown route or parameter, rejected by the loopback rules | none | 400, 403, 404, 415 |
| 10 | invalid schema or malformed input | `Schema error:` | 400 |
| 20 | cryptographic or chain failure | `Ledger error:` | 409, 422 |
| 20 | semantic rule violation | `Policy error:` | 409 |
| 30 | peer or network unavailable | `Network error:` | 502 |
| 50 | stale state or a conflicting event | `State error:` | 409 |
| 50 | replay of a single-use artifact | `Replay error:` | 409 |
| 60 | insecure key file permissions | none | 500 |
| 70 | unsupported feature or version | none | 501 |

## Verification reports

`mabel verify trust` and `mabel verify ledger` return these reports. There is
no HTTP route for them: proposal 004 removed `POST /api/verify` with the
verify tab, and verification stays a CLI concern. Every report carries
`source`, `head_seq`, `head_event` and `fetched_at_ms` (flag R, proposal 001
section 6), plus `sources_queried`, and a rendered `statement`:

```
valid as of seq 2 of <ledger id>, fetched from <endpoint id> at <RFC 3339>; no revocation up to seq 2
```

The revocation clause appears only in trust verification. It reads
`no revocation up to seq N` when nothing was revoked, and
`attestation <event id> revoked at seq M` for each revoked attestation
otherwise. `mabel verify ledger` renders the same sentence without any
revocation clause.

Trust reports also carry the flag-L sentence verbatim in `subject_control`:

```
subject control was not proven to this verifier; the issuer is responsible for out-of-band confirmation
```

and both report types carry the pitfall-8 sentence in `verified_means`.

`trusted` is pinned: one unrevoked attestation in `0..=head` gives
`trusted: true` with `attestation_event` and `attestation_seq`; otherwise
`trusted: false` with `revoked_count` and the revoked attestations. Both
exit 0. An unresolved subject also exits 0, with
`subject_resolution: "unresolved"` and the sentence in `subject_note`.

A trust report names who signed the attestation it answers with, as
`signing_principal: {identity, key}`: the principal identity the `author_key`
matched and that key, so a delegate's signature is not read as the subject's
(proposal 002 section 5). It is `null` when `trusted` is false.

Partial validity is a failure, not a result: `mabel verify ledger` on a
chain that breaks part way exits 20 with the error envelope, and the report
fields including `valid_to_seq` and `failed_at_seq` live in `details`.

## Shared documents

**Identity document**, returned by `GET /api/identities`, `GET
/api/identities/:identity_id`, `POST /api/identities` and `mabel identity
list`: `identity_id`, `declared_kind`, `alias`, `created_at_ms`, `head_seq`,
`head_event`, `event_count`, `witnesses`, `trust`, `principals`,
`open_invitation_count`, `profile`, `verification`, `contact`. A raw-rooted
identity adds `active_key` and `reserve_commit`; an identity-rooted one holds
no key of its own and omits both. The list route returns the same document as
the show route, not a truncated one: both parse into one type, with no key
present in one and absent in the other (proposal 003 section 5).

`profile` is the fold of the latest `ProfileUpdate`, or `null` on a ledger
that carries none: `display_name`, `hostname`, `email`, `signing_principal
{identity, key}`, `event`, `seq`. Any of the three fields may be `null` on its
own, since an update replaces the whole document and an omitted field clears
it. `email` is a claim and nothing more: no route checks that it is
deliverable, exactly as no route checks that a `display_name` is a real name.
The signing principal is who signed the update, which is not always the
ledger's own identity: any current controller may rename the ledger.

`verification` is the advisory DNS verdict of proposal 003 section 2, always
present: `hostname`, `status`, `checked_at_ms`, `last_verified_at_ms`,
`stale`, `detail`, `unreachable`. `status` is one of `verified`,
`mismatched`, `unverified`, `unreachable` and `unclaimed`, and is `unclaimed`
with every other key `null` when the profile names no hostname. A hostname
this node has never checked reads `unverified` with `checked_at_ms: null` and
`stale: true`. `unreachable` is `{checked_at_ms, detail}` for a failed
re-check kept beside a decisive result, `null` otherwise. The verdict never
gates ledger validity (decision 015).

`contact` is the local private note of proposal 003 section 1, `{nickname,
note, updated_at_ms}` or `null`. It lives in `contacts/<identity_id>.json`,
covers foreign identities as well as this node's own, and is never signed or
synced.

`GET /api/identities` is cache-only: it triggers no DNS lookup. `GET
/api/identities/:identity_id` answers from the same cache and starts at most
one background refresh when the entry is stale. `POST
/api/identities/:identity_id/verification` forces a check and waits for it.

**`ResolvedIdentity`**, returned everywhere a foreign identity renders:
`identity_id`, `display_name`, `email`, `alias`, `hostname`,
`verification_status`, `provenance`. `display_name` and `email` come from one
source, the profile the node holds or the profile the last crawl read, so an
identity card shows a known public email without a second request.
`provenance` is `profile`, `alias` or `none` and names which
source the label came from, in the resolution order of proposal 003 section
4: the profile display name, then the local alias or contact nickname, then
the truncated id. It appears in `paths` hops, lookup headings, the target's
trust list, the reverse list and the graph roots.

**Profile replacement.** `POST /api/identities/:identity_id/profile` takes a
body with all three keys, `display_name`, `hostname` and `email`, any of which
may be `null`. A body missing any key is refused with code 2 and reason
`missing_field`: no client half-specifies a replacement, because a partial
update over a whole-document payload is how a hostname disappears unnoticed.
A replacement whose effect equals the current folded profile is refused
before signing with code 20, `Policy error:` and reason
`no_op_profile_update`, and the effect covers all three fields. An `email` the
canonical scanner refuses answers code 10, `Schema error:` and reason
`invalid_email`, the spelling `test-vectors/rejections/` uses for the same
rule. `mabel profile replace` prints the before-and-after
diff and asks for confirmation unless `--yes` is given; with `--json` it
requires `--yes` and otherwise exits 2 with reason `confirmation_required`.

**Lookup.** `GET /api/lookup/:identity_id?from=<identity_id>` answers "how do
I know this identity" relative to one local root, over the live graph
generation. `from` defaults to the lowest local identity id. The document
carries `identity` and `from` as `ResolvedIdentity`, `degrees`, `paths` (up
to three shortest paths, each a list of hops), `trust`, `reverse`,
`equivocation`, `fetched_at_ms`, `stale`, `sync_id`, `last_sync_ms`,
`graph_stale`, `graph_truncated` and `truncated_by`. Each hop carries `from`,
`to`, `attestation_event`, and the `fetched_at_ms`, `stale` and
`equivocation` of the node it reaches. `reverse` is always
`{best_effort: true, entries}`: it answers who in this crawl attests to the
identity, never who trusts them in the world. An identity absent from the
graph is a 200 with `degrees: null` and an empty `paths` list, not a 404,
because "not in my crawl" is an answer.

**Known identities.** `GET /api/identities/known` returns `{identities}`: every
identity this home has a local record of and does not control, one row each.
The row is `identity_id`, `display_name`, `alias`, `email`, `hostname`,
`verification_status`, `declared_kind`, `stored`, `trusted`, `degrees` and
`head_seq`. The first six are the `ResolvedIdentity` fields, resolved by the
same code the lookup route uses, so a name here means what it means there.
`declared_kind` and `head_seq` come from the stored copy and are `null` when
this home stores no copy. `stored` says whether `ledgers/<identity_id>/` holds
one. `trusted` is true when any identity in this home holds an unrevoked
`TrustAttestation` naming this identity. `degrees` is the edge count from the
nearest crawl root in the stored generation, `null` when no crawl reached the
identity, so `null` means "not in my crawl", never "no relationship". The route
reads the home and the stored generation only: it opens no socket and queries
no DNS.

Three local sources merge into the row set, by identity id: ledgers under
`ledgers/` this home did not root, nodes of the stored crawl generation, and
ids that have nothing but a note under `contacts/`. Rows sort by ascending
`identity_id` as it is rendered, so a client can reproduce the order from the
document. An identity this wallet lists under `identities/`, which is every
identity it can sign for, is excluded: those are the rows `GET /api/identities`
serves.

**Graph.** `GET /api/graph` returns `{graph}`, `null` when no crawl has run in
this home, and `POST /api/graph/sync` runs one crawl and returns the same
object, never `null`. The object is `sync_id`, `last_sync_ms`, `depth`,
`roots`, `node_count`, `edge_count`, `fetch_count`, `truncated`,
`truncated_by` (`depth`, `nodes`, `fetches` or `time`, `null` when nothing was
cut), `equivocations` and `stale`. Synchronizing is manual: there is no
background timer.

`principals` is the folded principal set of proposal 002 section 1, on every
ledger, raw-rooted or identity-rooted, sorted by ascending `identity`. Each
entry carries `identity`, `active_key`, `role` (`member` or `controller`) and
`is_root`, which is true for the principal the inception seeded.
`open_invitation_count` counts the invitations still `open`; the invitations
themselves are on `GET /api/identities/:identity_id/memberships`.

**Event document**, returned by both ledger routes, nested in fork records and
returned beside every append: `event_id`, `seq`, `ledger_id`, `prev`,
`timestamp_ms`, `author_key`, `payload_kind`, `payload`. `payload_kind` is the
`oneof` tag name from `ledger.proto` in snake_case, and `payload` holds that
variant's fields with the same names. The node decodes; the UI holds no keys
and does no crypto (proposal 001 section 10), so no raw event bytes are served
over HTTP. Raw bytes cross machines over Iroh and through the file artifacts.

The payload subtree is frozen. Eight `payload_kind` values exist, and each
`payload` holds exactly these keys:

| `payload_kind` | `payload` keys |
|---|---|
| `inception` | `declared_kind`, `nonce`, `root` |
| `witness_config` | `witnesses` |
| `trust_attestation` | `subject` |
| `trust_revocation` | `target` |
| `membership_invitation` | `invitee`, `invitee_key`, `role`, `invitee_inception` |
| `membership_acceptance` | `acceptance`, `signature` |
| `membership_removal` | `target` |
| `profile_update` | `display_name`, `hostname`, `email` |

`profile_update` replaces the whole profile: an omitted field is one the
update cleared, and all three keys are present and `null` in the `payload`
object when the event carries none (proposal 003 section 1, proposal 005).

`root` is the inception's root `oneof` (proposal 002 section 2), one key of
`raw_root` (`active_key`, `reserve_commit`) or `identity_root` (`founder`,
`founder_key`, `founder_inception`). The blobs an event embeds verbatim,
`founder_inception`, `invitee_inception`, `acceptance` and its `signature`,
render as base32 of those bytes and not as decoded messages: they are signed
objects, and a reader that wants their contents asks for the ledger they came
from.

Fixtures use the identities and event ids from `test-vectors/`, so a fixture
and a golden vector name the same event: Alice is
`sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq`, Bob is
`jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq`, the organization is
`2okqwhextnpkpmydrgrkk563vbehcklffwfzidxlh5dslawjmn6a`. Endpoint ids, Bob's
active key, the one conflicting fork event and the membership events are
fabricated but consistent across files.

Each fixture is one moment, not one snapshot of a single node.
`wallet-post-identities.json` answers the moment Alice's ledger is two events
long, the inception and the `ProfileUpdate` the create named;
`wallet-post-trust.json` answers at seq 2 of Alice's ledger;
`wallet-get-identity-ledger.json` reads it at head 3, the revocation of that
attestation; `wallet-get-identity.json` reads it at head 8, with the profile
at seq 7 and a second attestation at seq 8, and Alice's entry in
`wallet-get-identities.json` carries that same head. The five membership
fixtures sit
between those two heads, Alice delegating to Bob on her own raw-rooted ledger:
the invitation lands at seq 4, the acceptance at seq 5 and the removal at seq
6, and `wallet-get-identity-memberships.json` reads that ledger at head 4,
with the invitation still open. The acceptance fixture is
the mirror image, Alice's wallet accepting an invitation to Bob's ledger,
because a wallet only signs an acceptance for an identity whose key it holds.

## Membership

Membership is legal on every ledger (proposal 002 section 4), so the routes
hang off an identity and no `/orgs` or `/organizations` route exists. The path
parameter is the ledger the event lands in, except on `/acceptances`, where it
is the invitee signing on their own wallet.

| Method | Route | Who runs it | Body |
|---|---|---|---|
| GET | `/api/identities/:identity_id/memberships` | anyone holding the ledger | none |
| POST | `/api/identities/:identity_id/memberships/invitations` | a controller of the ledger | `by`, `role`, `invitee_descriptor_base64` |
| POST | `/api/identities/:identity_id/memberships/acceptances` | the invitee | `invitation_bundle_base64` |
| POST | `/api/identities/:identity_id/memberships/admissions` | a controller of the ledger | `by`, `acceptance_base64` |
| POST | `/api/identities/:identity_id/memberships/removals` | a controller of the ledger | `by`, `target` |

One admission crosses two wallets and three routes. A controller posts an
invitation and gets back `invitation_bundle_base64`, the ledger's events
`0..=invitation`. The invitee posts that bundle to `/acceptances` on their own
wallet: the node folds it, answers the accept surface below, signs the
acceptance and returns it as `acceptance_base64`. A controller posts that to
`/admissions`, which appends it and adds the principal. Nobody is added
without their own signature (decision 004), and the node signs, never the
browser (proposal 001 section 10).

`/acceptances` and `/admissions` are different actions on different wallets:
accepting an invitation you received signs a detached file and appends
nothing, while admitting someone's acceptance appends to your ledger. Naming
both `/acceptances` would hide that difference behind one path.

**Accept surface** (proposal 002 section 4), the response of `/acceptances`
beside `acceptance_base64`: `ledger_id`, `declared_kind`, `root` (`raw` or
`identity`), `controllers`, `invitation_event`, `invitee`, `invitee_key`,
`role`, `controller_on_raw_root` and `warning`. `controller_on_raw_root` is
the flag a screen branches on; `warning` is the sentence a person reads, and
is `null` exactly when the flag is false. Accepting a `controller` role on a
raw-rooted ledger means signing as that identity, which is what the sentence
says.

**Membership document**, returned by `GET
/api/identities/:identity_id/memberships` and by `mabel membership list
--json`: `ledger_id`, `declared_kind`, `root`, `head_seq`, `head_event`,
`principals` and `invitations`. Each invitation carries `invitation_event`,
`invitation_seq`, `invitee`, `invitee_key`, `role` and `status` (`open`,
`accepted` or `cancelled`); accepted and cancelled invitations stay in the
list, sorted by ascending `invitation_seq`.

`POST /api/identities` takes an optional `founder`. Present, it names the one
founding principal of an identity root and the new ledger holds no key of its
own; absent or `null`, the ledger keys itself with a raw root (proposal 002
section 2). The request keeps the frozen `declared_kind` spelling, which
proposal 002 section 6 writes as `kind`: the fixture name wins.

`POST /api/identities` also takes an optional `display_name` and an optional
`email`. Either one given, the node appends one `ProfileUpdate` at seq 1 right
after the inception, and the identity document it answers with reports
`head_seq: 1`, `event_count: 2` and that profile. Neither given, both may be
absent or `null` and the new ledger is one event long with `profile: null`.
The create takes no `hostname`: a hostname is a DNS claim with a verification
cycle behind it, and `POST /api/identities/:identity_id/profile` is where it
is made. Unlike that route, the create keys are optional rather than
required: it publishes a profile, it does not replace one, so there is nothing
an absent key could silently clear.

Replaying an acceptance the ledger already admitted answers 409 with `code:
50`, `Replay error:` and `reason: acceptance_already_used`, the case
`contracts/cli/errors.json` pins for the CLI. The fold calls that state
`invitation_not_open`, which is true but says nothing about the file the
caller passed.

## Decisions taken here

Each of these settles something proposal 001 leaves open. Flagged so a
reviewer can overrule them cheaply, before consumers are written.

- Success documents carry `ok: true`, so one key discriminates success from
  the error envelope on both surfaces. Proposal 001 fixes `{ok, code,
  message, details}` for errors only.
- HTTP errors reuse the CLI exit code as `code` alongside the HTTP status.
- Every successful route answers 200, including `POST /api/identities`,
  rather than 201.
- Public keys and endpoint ids render as base32 like ids, not as
  `iroh_base`'s hex. `node.json` keeps hex, so the HTTP layer converts.
- `GET /api/node` calls the storage limit `storage_capacity`, and so does
  `NodeConfig` in `crates/mabel-node/src/config.rs`: the config field was
  renamed rather than mapped, and a `node.json` spelling it `storage_cap` is
  refused as an unknown field.
- The witness JSON renames `LedgerSummary.ledger` to `ledger_id` and
  `LedgerSummary.kind` to `declared_kind`, and adds `source_endpoint` from
  `ledgers/<id>/meta.json`. Every other field keeps its `sync.proto` name.
- Route path parameters are named in full: `:identity_id`, `:ledger_id`,
  `:event_id`. The URL shapes are unchanged from proposal 001 section 10.
- `POST /api/trust/:event_id/revoke` takes a body naming the `issuer`,
  because mutating routes must send `content-type: application/json` anyway
  and the node then needs no event-id-to-ledger index.
- A push where at least one witness accepted exits 0 and reports the
  failures per endpoint in `results`; a push where every witness failed
  exits 30.
- The three loopback rules reject with code 2 and no layer prefix, at 403
  for a bad `Host` or `Origin` and 415 for a missing content type. The
  `host_not_loopback` and `origin_mismatch` messages the `http/*.json` fixtures
  pin are the default set, loopback alone. An operator who passes
  `--allow-host` widens both sets, and the message then lists what it accepts:
  `Host header must be 127.0.0.1:9080, localhost:9080 or wallet.example`
  (decision 018).
- Codes 60 and 70 carry no layer prefix, since the six prefixes proposal 001
  lists map onto 10, 20, 30 and 50.
- `details.reason` is a stable snake_case class name, matching how
  `test-vectors/rejections/` already treats `code`.
- File artifacts cross JSON as base64 in a `*_base64` field, while every other
  byte field stays base32. Two spellings, one rule: base32 for the 32-byte
  values a person compares, base64 for the blobs a person never reads.
- The membership routes name the ledger in the path and never repeat it in the
  body, as `POST /api/identities/:identity_id/witnesses` already does.
- `by` is required on every membership route that appends. A raw-rooted ledger
  has an obvious signer, an identity-rooted one does not, and one shape covers
  both.
- Admitting an acceptance is `POST .../memberships/admissions`, not a second
  meaning for `/acceptances`. Proposal 002 section 6 lists three routes and
  leaves the fourth verb unnamed.
- The replay case in `contracts/cli/errors.json` spells its detail key
  `invitation_event`, not `invite_event`: decision 012 forbids the
  abbreviation and proposal 002 spells the event `invitation` everywhere.
- `no_op_profile_update` is code 20 with the `Policy error:` prefix at 409.
  It is a semantic rule the node enforces before signing, which is the row
  code 20 already names; code 2 would say the request was malformed, and it
  is not.
- The contact routes hang off the identity, `GET` and `PUT
  /api/identities/:identity_id/contact`, matching the fixture names proposal
  003 section 5 lists. `PUT` is the only non-`POST` mutating verb in the API:
  the contact document is replaced whole, at a path that names it.
- `ResolvedIdentity` spells its verdict `verification_status` and carries the
  status string alone, not the whole verification object. Proposal 003
  section 4 writes the key as `verification`; a foreign identity in a path hop
  needs the glyph, not the seven fields of that object, and the full object is
  one route away.
- The identity document's `verification` carries `unreachable`, which
  proposal 003 section 5 does not list. Section 2 requires the document to
  report a failed re-check beside the decisive result it could not refresh,
  and this is where it goes.
- A claimed hostname this node has never checked reads `status: "unverified"`
  with `checked_at_ms: null`. The five statuses are frozen and none of them
  means "not checked yet"; the null timestamp is what says so, and the UI
  renders `unverified` dimmed either way.
- `GET /api/lookup/:identity_id` defaults `from` to the lowest local identity
  id. Proposal 003 section 3 defaults it to the identity selected in the
  wallet, which is a browser fact the node does not hold; a client that cares
  sends the parameter.
- `GET /api/witnesses/:identity_id/holdings` names its array `ledgers`, not
  `entries`, and each row carries six keys: `ledger_id`, `declared_kind`,
  `head_seq`, `head_event`, `event_count` and `fork_count`. The row is what
  the `List` request of `sync.proto` serves, so `source_endpoint`,
  `first_seen_ms` and `forks_truncated` cannot appear: they come from the
  answering node's own `ledgers/<id>/meta.json`, which no peer sends. The last
  segment is `holdings` and the key is an identity id (proposal 006 section 8);
  an id equal to an endpoint id this home knows answers 404 with reason
  `endpoint_not_identity`, before any dial.
- A witness that cannot be dialled, or that refuses the `List`, answers 502
  with code 30 and reason `witness_unreachable`, naming the identity in
  `details.identity_id` and every endpoint dialled in
  `details.endpoints_tried`. A fetch that named a bare endpoint keeps
  `details.endpoint_id`, because that caller named a machine and no identity.
- `GET /api/resolve?input=` takes one identity id, one hostname or one
  `mabel://` link and says which it read in `input_kind` (`identity`,
  `hostname` or `link`). It writes nothing: it never reads or fills the
  verification cache of proposal 003 section 2. Navigation is not
  verification, and a hostname typed into a search box is not a claim any
  ledger made. Its four statuses (`resolved`, `no_record`,
  `mismatched_records`, `unreachable`) are a separate vocabulary from the five
  of `verification.status`, and `status` is `null` on the two kinds that query
  nothing. `endpoints` holds the link's hints, or the `mabel-endpoints=`
  records at the label a hostname resolved to, sorted ascending by rendered
  base32. The route decodes `input` exactly once and the link grammar refuses
  percent-encoding, so `%252f` is refused with code 2 and reason
  `invalid_mabel_link` rather than decoded twice; a repeated or unknown query
  key is `unknown_query_parameter` (proposal 006 sections 6 and 7).
- `POST /api/identities/:identity_id/fetch` answers the document
  `contracts/cli/sync-fetch.json` pins for `mabel sync fetch --json`, because
  it is the same operation over the same wallet core. `from` names one endpoint
  and is a plain `CallerHint`: an endpoint this wallet has never heard of is
  dialled anyway, because a human named it for this request. `from_witness`
  names one witness identity and is resolved to endpoints through proposal 006
  section 5.1, with `unresolvable_witness` when this home can reach none of
  them. Both keys at once is code 2 and reason `conflicting_source`, before
  anything is dialled.
- `node.json.witnesses` holds `{identity, endpoints}` objects (proposal 006
  section 5.4). An array of 64-character hex endpoint ids is the
  pre-proposal-006 shape and fails to load, naming
  `mabel witness set-default --witness <mabel-id> --endpoints <endpoint,...>`;
  a bare 52-character identity id loads as an entry with no bootstrap
  endpoints. `peers.json` holds
  `{endpoint, first_seen_ms, last_success_ms, failures}` objects, at most 8 per
  ledger, and a bare endpoint id still loads as a hint with no timestamps
  (section 5.3).
- Proposal 005 amends the payload-table freeze a second time: the
  `profile_update` row gains `email`, and every fixture that renders a profile
  object, a `ResolvedIdentity` or a `previous` profile carries the key with an
  explicit `null` where nothing is claimed. Ticket 023 amended the same table
  by adding the row; this adds a key to it. No frozen event id changes,
  because an absent field encodes no bytes: the golden vectors 12 to 14 keep
  their `body_hex` and only their rendered `body` gains `"email": null`.
- `wallet-post-identities.json` now answers with a profile at seq 1, so its
  frozen `head_seq`, `head_event` and `event_count` moved from 0, the identity
  id and 1 to 1, a fabricated `ProfileUpdate` event id and 2. The plain create
  that publishes nothing stays pinned, on the CLI side, as the `created` case
  of `contracts/cli/identity-create.json`, beside the new
  `created-with-a-profile` case. One HTTP fixture holds one example, and the
  example worth freezing is the one with the new keys in the request.
- The public email is `email` on every surface, never `contact_email` or
  `public_email`. The profile has one email, the private note in `contact`
  has none, and decision 012 forbids a qualifier that carries no information.
- `GET /api/identities/known` sorts by ascending `identity_id` alone, not by
  whether a row carries a `display_name`. The list is a set of ids, an id is
  the only stable key, and a client that wants named rows first sorts them
  itself; a server-side name sort would reorder the list every time a crawl
  reads a new profile. Ascending means ascending in the rendered base32, which
  puts digits before letters, and not ascending in the 32 bytes behind it: the
  two orders differ, and only the first is one a client can check.
- The known-identity row flattens the six `ResolvedIdentity` fields and drops
  `provenance`. The row already carries `display_name` and `alias`, which is
  what `provenance` is computed from, and it carries five more fields of its
  own; nesting the shared object would put half a row one level down.
- `known` is a static segment under `/api/identities`, so no identity id can
  collide with it: an id is 52 base32 characters. `GET
  /api/identities/known` is matched before `GET
  /api/identities/:identity_id`.
- `wallet-get-known-identities.json` pins two rows, Bob stored and trusted at
  one degree and Carol crawl-only at two. It pins no row with `degrees: null`,
  the contact note for an identity no crawl reached, because the fixture
  vocabulary holds no fourth foreign identity to name one with; the case is
  covered in `crates/mabel-node/tests/profile_graph.rs`.
- A wallet route asked for an identity this home does not hold answers 404
  with reason `unknown_ledger`, detail key `ledger_id` and the message `this
  home holds no ledger <id>`. One spelling covers every wallet route, the
  identity ones and the ledger ones, because an identity in this home is the
  ledger it roots. `unknown_ledger` is the one spelling on every node:
  `ledger_not_held` died with the witness routes (proposal 006 section 8).
