// Types transcribed by hand from contracts/http/*.json and contracts/cli/*.json.
// The fixtures are normative: a shape that disagrees with them is a bug here.
// Field names are snake_case with full words (decision 012), byte fields are
// lowercase base32 strings, sequences are 0-based numbers, timestamps are unix
// milliseconds named *_ms, and a field that does not apply is null, not absent.

/** contracts/README.md, "Declared kind". Advisory, gates nothing (proposal 002 section 3). */
export type DeclaredKind = "person" | "organization" | "agent" | "service";

/** contracts/README.md, "The envelope". Consumers branch on code and details.reason. */
export interface ErrorEnvelope {
  ok: false;
  code: number;
  message: string;
  details: ErrorDetails;
}

export interface ErrorDetails {
  reason: string;
  [key: string]: unknown;
}

/**
 * payload_kind is the oneof tag name from ledger.proto in snake_case. The seven
 * frozen values are listed in contracts/README.md, "Event document": inception,
 * witness_config, trust_attestation, trust_revocation, membership_invitation,
 * membership_acceptance and membership_removal. It stays a string so a node
 * that mints a value this build does not know still renders.
 */
export type PayloadKind = string;

/** contracts/README.md, "Event document". */
export interface LedgerEvent {
  event_id: string;
  seq: number;
  ledger_id: string | null;
  prev: string | null;
  timestamp_ms: number;
  author_key: string;
  payload_kind: PayloadKind;
  payload: Record<string, unknown>;
}

/** One entry of Identity.trust. */
export interface TrustRecord {
  attestation_event: string;
  attestation_seq: number;
  subject: string;
  revoked: boolean;
  revocation_event: string | null;
  revocation_seq: number | null;
}

/** What a principal may do (proposal 002 section 1). */
export type Role = "member" | "controller";

/**
 * One entry of Identity.principals, contracts/README.md, "Identity document".
 * is_root is true for the principal the inception seeded.
 */
export interface PrincipalEntry {
  identity: string;
  active_key: string;
  role: Role;
  is_root: boolean;
}

/**
 * The two fields a ProfileUpdate carries (proposal 003 section 1). The payload
 * replaces the whole document, so an omitted field clears that name; both the
 * request body and the before-and-after diff use this shape.
 */
export interface ProfileFields {
  display_name: string | null;
  hostname: string | null;
}

/**
 * Identity.profile: the fold of the latest ProfileUpdate, null on a ledger that
 * carries none. signing_principal is who signed it, which is not always the
 * ledger's own identity: any current controller may rename the ledger.
 */
export interface Profile extends ProfileFields {
  signing_principal: SigningPrincipal;
  event: string;
  seq: number;
}

/**
 * The five advisory DNS verdicts of proposal 003 section 2. unclaimed means the
 * profile names no hostname; unverified also covers a hostname this node has
 * never checked, which reads checked_at_ms: null.
 */
export type VerificationStatus =
  | "verified"
  | "mismatched"
  | "unverified"
  | "unreachable"
  | "unclaimed";

/** A failed re-check kept beside a decisive result, so both timestamps show. */
export interface UnreachableRecheck {
  checked_at_ms: number;
  detail: string | null;
}

/**
 * Identity.verification, always present. The verdict is advisory and gates no
 * ledger validity (decision 015). stale marks a verified result older than 24
 * hours, which is never rendered as a plain check.
 */
export interface Verification {
  hostname: string | null;
  status: VerificationStatus;
  checked_at_ms: number | null;
  last_verified_at_ms: number | null;
  stale: boolean;
  detail: string | null;
  unreachable: UnreachableRecheck | null;
}

/**
 * Identity.contact: the local private note of proposal 003 section 1, held in
 * contacts/<identity_id>.json. It covers foreign identities too, and is never
 * signed and never synced.
 */
export interface Contact {
  nickname: string | null;
  note: string | null;
  updated_at_ms: number;
}

/**
 * contracts/README.md, "Identity document". active_key and reserve_commit are
 * the root-dependent exception to the nullability rule: a raw-rooted identity
 * carries both, an identity-rooted one holds no key of its own and omits them.
 * principals is the folded principal set, on every ledger of either root.
 */
export interface Identity {
  identity_id: string;
  declared_kind: DeclaredKind;
  alias: string;
  created_at_ms: number;
  head_seq: number;
  head_event: string;
  event_count: number;
  witnesses: string[];
  trust: TrustRecord[];
  principals: PrincipalEntry[];
  open_invitation_count: number;
  profile: Profile | null;
  verification: Verification;
  contact: Contact | null;
  active_key?: string;
  reserve_commit?: string;
}

/** Which source a resolved name came from, in the order section 4 fixes. */
export type NameProvenance = "profile" | "alias" | "none";

/**
 * contracts/README.md, "ResolvedIdentity": the object returned everywhere a
 * foreign identity renders. It carries the verdict as a status string alone,
 * spelled verification_status, because a path hop needs the glyph and not six
 * timestamps.
 */
export interface ResolvedIdentity {
  identity_id: string;
  display_name: string | null;
  alias: string | null;
  hostname: string | null;
  verification_status: VerificationStatus;
  provenance: NameProvenance;
}

/** One branch of an equivocation: the source that served it and the event. */
export interface EquivocationBranch {
  source: { kind: string; endpoint: string };
  event: string;
}

/** Two signed events at one seq, recorded by the crawl (proposal 003 section 3). */
export interface Equivocation {
  at_seq: number;
  branches: EquivocationBranch[];
}

/** One edge of a lookup path, carrying the freshness of the node it reaches. */
export interface LookupHop {
  from: ResolvedIdentity;
  to: ResolvedIdentity;
  attestation_event: string;
  fetched_at_ms: number;
  stale: boolean;
  equivocation: Equivocation | null;
}

export interface LookupPath {
  hops: LookupHop[];
}

/** One outgoing attestation of the looked-up identity. */
export interface LookupTrustEntry {
  subject: ResolvedIdentity;
  attestation_event: string;
  seq: number;
}

/** One incoming attestation this crawl happens to hold. */
export interface LookupReverseEntry {
  identity: ResolvedIdentity;
  attestation_event: string;
  seq: number;
}

/**
 * best_effort is always true: the reverse list answers who in this crawl
 * attests to the identity, never who trusts them in the world.
 */
export interface LookupReverse {
  best_effort: true;
  entries: LookupReverseEntry[];
}

/** What the crawl stopped for, null when nothing was cut. */
export type TruncatedBy = "depth" | "nodes" | "fetches" | "time";

/**
 * GET /api/lookup/:identity_id?from=. An identity absent from the graph is a
 * 200 with degrees null and an empty path list: "not in my crawl" is an answer.
 */
export interface LookupResponse {
  ok: true;
  identity: ResolvedIdentity;
  from: ResolvedIdentity;
  degrees: number | null;
  paths: LookupPath[];
  trust: LookupTrustEntry[];
  reverse: LookupReverse;
  equivocation: Equivocation | null;
  fetched_at_ms: number | null;
  stale: boolean;
  sync_id: string | null;
  last_sync_ms: number | null;
  graph_stale: boolean;
  graph_truncated: boolean;
  truncated_by: TruncatedBy | null;
}

/** One crawl generation, described by contracts/README.md, "Graph". */
export interface Graph {
  sync_id: string;
  last_sync_ms: number;
  depth: number;
  roots: ResolvedIdentity[];
  node_count: number;
  edge_count: number;
  fetch_count: number;
  truncated: boolean;
  truncated_by: TruncatedBy | null;
  equivocations: string[];
  stale: boolean;
}

/** GET /api/graph. graph is null when no crawl has run in this node home. */
export interface GraphResponse {
  ok: true;
  graph: Graph | null;
}

/** POST /api/graph/sync runs one crawl, so its graph is never null. */
export interface GraphSyncResponse {
  ok: true;
  graph: Graph;
}

/**
 * POST /api/identities/:identity_id/profile. Both keys are required and either
 * may be null: a body missing one is refused with reason missing_field, because
 * a partial update over a whole-document payload is how a hostname disappears.
 */
export type ReplaceProfileRequest = ProfileFields;

export interface ReplaceProfileResponse {
  ok: true;
  ledger_id: string;
  profile: Profile;
  /** The profile as it was, which is what the confirmation diff shows. */
  previous: ProfileFields;
  head_seq: number;
  head_event: string;
  event: LedgerEvent;
}

/** POST /api/identities/:identity_id/verification forces a check and waits. */
export interface VerificationResponse {
  ok: true;
  identity_id: string;
  verification: Verification;
}

/** GET and PUT /api/identities/:identity_id/contact, valid for foreign ids too. */
export interface ContactResponse {
  ok: true;
  identity_id: string;
  contact: Contact | null;
}

export interface SetContactRequest {
  nickname: string | null;
  note: string | null;
}

/** GET /api/node on a wallet. */
export interface WalletNodeInfo {
  ok: true;
  role: "wallet";
  endpoint_id: string;
  http_bind: string;
  relay: string;
  witnesses: string[];
  storage_capacity: number;
  storage_used: number;
  identity_count: number;
  version: string;
}

/** GET /api/node on a witness. */
export interface WitnessNodeInfo {
  ok: true;
  role: "witness";
  endpoint_id: string;
  http_bind: string;
  relay: string;
  witnesses: string[];
  storage_capacity: number;
  storage_used: number;
  ledger_count: number;
  fork_count: number;
  version: string;
}

/** GET /api/identities. Sorted by ascending identity_id. */
export interface IdentityListResponse {
  ok: true;
  identities: Identity[];
}

/** GET /api/identities/:identity_id. */
export interface IdentityResponse {
  ok: true;
  identity: Identity;
}

/**
 * POST /api/identities. The frozen request is {alias, declared_kind}; founder
 * is the proposal 002 section 6 addition that selects an identity root, and is
 * omitted for a raw root.
 */
export interface CreateIdentityRequest {
  alias: string;
  declared_kind: DeclaredKind;
  founder?: string;
}

export interface CreateIdentityResponse {
  ok: true;
  identity: Identity;
  inception_event: string;
}

/** GET /api/identities/:identity_id/ledger?since=&limit=. since is inclusive. */
export interface LedgerPageResponse {
  ok: true;
  ledger_id: string;
  declared_kind: DeclaredKind;
  since: number;
  limit: number;
  head_seq: number;
  head_event: string;
  event_count: number;
  more: boolean;
  events: LedgerEvent[];
}

/** POST /api/identities/:identity_id/witnesses and POST /api/trust. */
export interface AppendResponse {
  ok: true;
  ledger_id: string;
  head_seq: number;
  head_event: string;
  event: LedgerEvent;
}

export interface SetWitnessesRequest {
  witnesses: string[];
}

export interface AddTrustRequest {
  issuer: string;
  subject: string;
}

/** POST /api/trust/:event_id/revoke. */
export interface RevokeTrustRequest {
  issuer: string;
}

export interface RevokeTrustResponse {
  ok: true;
  ledger_id: string;
  head_seq: number;
  head_event: string;
  revoked_attestation: string;
  revoked_attestation_seq: number;
  event: LedgerEvent;
}

/** RejectCode from proto/mabel/v0/sync.proto, rendered as its enum name. */
export type RejectCode =
  | "MALFORMED"
  | "TOO_LARGE"
  | "INVALID"
  | "FORK"
  | "UNSUPPORTED"
  | "NOT_ADMITTED"
  | "BUSY";

/**
 * One witness outcome of POST /api/sync/push. The fixtures show accepted and
 * unreachable; rejected is the third state reject_code and at_seq describe.
 */
export interface PushResult {
  endpoint: string;
  status: "accepted" | "rejected" | "unreachable";
  head_seq: number | null;
  stored: number;
  reject_code: RejectCode | null;
  at_seq: number | null;
  message: string | null;
}

export interface SyncPushRequest {
  identity_id: string;
  /** null pushes to every configured witness, or one endpoint id to pin a target. */
  to: string | null;
}

export interface SyncPushResponse {
  ok: true;
  ledger_id: string;
  head_seq: number;
  head_event: string;
  results: PushResult[];
}

export interface VerifyTrustRequest {
  kind: "trust";
  issuer: string;
  subject: string;
  /** null queries every configured source, or one endpoint id to pin a source. */
  from: string | null;
}

export interface VerifyLedgerRequest {
  kind: "ledger";
  ledger_id: string;
  from: string | null;
}

export type VerifyRequest = VerifyTrustRequest | VerifyLedgerRequest;

/** Fields every report carries (flag R, proposal 001 section 6). */
export interface ReportProvenance {
  source: string;
  sources_queried: string[];
  head_seq: number;
  head_event: string;
  fetched_at_ms: number;
  statement: string;
  verified_means: string;
}

export interface RevokedAttestation {
  attestation_event: string;
  attestation_seq: number;
  revocation_event: string;
  revocation_seq: number;
}

/** POST /api/verify with kind trust, and mabel verify trust --json. */
export interface VerifyTrustReport extends ReportProvenance {
  ok: true;
  kind: "trust";
  trusted: boolean;
  issuer: string;
  subject: string;
  subject_resolution: "resolved" | "unresolved";
  subject_note: string | null;
  attestation_event: string | null;
  attestation_seq: number | null;
  revoked_count: number;
  revoked_attestations: RevokedAttestation[];
  /** Flag L, verbatim. */
  subject_control: string;
  /** The author_key and the principal it matched (proposal 002 section 5). */
  signing_principal?: SigningPrincipal | null;
}

/** The principal whose key signed an event (proposal 002 section 5). */
export interface SigningPrincipal {
  identity: string;
  key: string;
}

/** POST /api/verify with kind ledger, and mabel verify ledger --json. */
export interface VerifyLedgerReport extends ReportProvenance {
  ok: true;
  kind: "ledger";
  ledger_id: string;
  declared_kind: DeclaredKind;
  valid: boolean;
  valid_to_seq: number;
  failed_at_seq: number | null;
  event_count: number;
  signing_principal?: SigningPrincipal | null;
}

export type VerifyReport = VerifyTrustReport | VerifyLedgerReport;

/** GET /api/ledgers and GET /api/ledgers/:ledger_id on a witness. */
export interface LedgerSummary {
  ledger_id: string;
  declared_kind: DeclaredKind;
  head_seq: number;
  head_event: string;
  event_count: number;
  first_seen_ms: number;
  updated_ms: number;
  fork_count: number;
  forks_truncated: boolean;
  source_endpoint: string;
}

export interface LedgerListResponse {
  ok: true;
  offset: number;
  limit: number;
  more: boolean;
  entries: LedgerSummary[];
}

export interface LedgerEntryResponse {
  ok: true;
  entry: LedgerSummary;
  witnesses: string[];
}

export interface ForkRecord {
  ledger_id: string;
  seq: number;
  observed_ms: number;
  source_endpoint: string;
  kept: LedgerEvent;
  conflicting: LedgerEvent;
  statement: string;
}

export interface ForkListResponse {
  ok: true;
  offset: number;
  limit: number;
  more: boolean;
  entries: ForkRecord[];
}
