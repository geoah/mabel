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
  active_key?: string;
  reserve_commit?: string;
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
