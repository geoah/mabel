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

## Index

| File | Surface |
|---|---|
| `http/wallet-get-node.json` | `GET /api/node` (wallet) |
| `http/wallet-get-identities.json` | `GET /api/identities` |
| `http/wallet-post-identities.json` | `POST /api/identities` |
| `http/wallet-get-identity.json` | `GET /api/identities/:identity_id` |
| `http/wallet-get-identity-ledger.json` | `GET /api/identities/:identity_id/ledger?since=` |
| `http/wallet-post-identity-witnesses.json` | `POST /api/identities/:identity_id/witnesses` |
| `http/wallet-post-trust.json` | `POST /api/trust` |
| `http/wallet-post-trust-revoke.json` | `POST /api/trust/:event_id/revoke` |
| `http/wallet-post-sync-push.json` | `POST /api/sync/push` |
| `http/wallet-post-verify.json` | `POST /api/verify` |
| `http/witness-get-node.json` | `GET /api/node` (witness) |
| `http/witness-get-ledgers.json` | `GET /api/ledgers` |
| `http/witness-get-ledger.json` | `GET /api/ledgers/:ledger_id` |
| `http/witness-get-ledger-events.json` | `GET /api/ledgers/:ledger_id/events?since=` |
| `http/witness-get-forks.json` | `GET /api/forks` |
| `http/PENDING-membership.md` | the four membership routes, not frozen yet |
| `cli/identity-create.json` | `mabel identity create --json` |
| `cli/identity-list.json` | `mabel identity list --json` |
| `cli/trust-add.json` | `mabel trust add --json` |
| `cli/sync-push.json` | `mabel sync push --json` |
| `cli/verify-trust.json` | `mabel verify trust --json` |
| `cli/verify-ledger.json` | `mabel verify ledger --json` |
| `cli/errors.json` | the error envelope, one case per exit code and layer prefix |

Each `http/*.json` holds `route`, `method`, `request` (an example body, or
`null` for GET), `response` (an example 200 body) and `errors` (examples of
`{status, body}`). Each `cli/*.json` holds `command` and `cases`, one case
per outcome worth pinning, each with `case`, `command`, `exit_code` and
`document`.

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
`iroh_base::EndpointId` serializes as hex there.

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

**Ordering.** `GET /api/identities` and `GET /api/ledgers` sort by ascending
id, matching the `List` request in `sync.proto`, so paging is stable. Events
sort by ascending `seq`.

**Paging.** `offset`, `limit` and `more` on every paged route, echoed back in
the response. `?since=` is inclusive: the response starts at `seq == since`
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

`POST /api/verify` and `mabel verify ...` return the same report. Every
report carries `source`, `head_seq`, `head_event` and `fetched_at_ms` (flag
R, proposal 001 section 6), plus `sources_queried`, and a rendered
`statement`:

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

Partial validity is a failure, not a result: `mabel verify ledger` on a
chain that breaks part way exits 20 with the error envelope, and the report
fields including `valid_to_seq` and `failed_at_seq` live in `details`.

## Shared documents

**Identity document**, returned by `GET /api/identities`, `GET
/api/identities/:identity_id`, `POST /api/identities` and `mabel identity
list`: `identity_id`, `declared_kind`, `alias`, `created_at_ms`, `head_seq`,
`head_event`, `event_count`, `witnesses`, `trust`. A person adds
`active_key` and `reserve_commit`. The list route returns the same document
as the show route, not a truncated one. The membership view of an identity
is not frozen; see below.

**Event document**, returned by both ledger routes and nested in fork
records: `event_id`, `seq`, `ledger_id`, `prev`, `timestamp_ms`,
`author_key`, `payload_kind`, `payload`. `payload_kind` is the `oneof` tag
name from `ledger.proto` in snake_case, and `payload` holds that variant's
fields with the same names. The node decodes; the UI holds no keys and does
no crypto (proposal 001 section 10), so no raw event bytes are served over
HTTP. Raw bytes cross machines over Iroh and through the file artifacts.

Fixtures use the identities and event ids from `test-vectors/`, so a fixture
and a golden vector name the same event: Alice is
`sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq`, Bob is
`jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq`, the organization is
`2okqwhextnpkpmydrgrkk563vbehcklffwfzidxlh5dslawjmn6a`. Endpoint ids and the
one conflicting fork event are fabricated but consistent across files.

## Not frozen

The four membership routes (`POST /api/orgs`, `/orgs/:id/invites`,
`/orgs/:id/acceptances`, `/orgs/:id/removals`), the membership fields of the
identity document, and the `payload_kind` names of membership and inception
events all wait on proposal 002. See
[http/PENDING-membership.md](http/PENDING-membership.md).

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
- `GET /api/node` calls the storage limit `storage_capacity`;
  `NodeConfig` in `crates/mabel-node/src/config.rs` spells it `storage_cap`.
  Either the API maps the name or the config field is renamed.
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
  for a bad `Host` or `Origin` and 415 for a missing content type.
- Codes 60 and 70 carry no layer prefix, since the six prefixes proposal 001
  lists map onto 10, 20, 30 and 50.
- `details.reason` is a stable snake_case class name, matching how
  `test-vectors/rejections/` already treats `code`.
