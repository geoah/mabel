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
 * payload_kind is the oneof tag name from ledger.proto in snake_case. The frozen
 * values are listed in contracts/README.md, "Event document": inception,
 * witness_config, trust_attestation, trust_revocation, membership_invitation,
 * membership_acceptance, membership_removal, profile_update, witness_set and
 * endpoint_advertisement. It stays a string so a node that mints a value this
 * build does not know still renders.
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

/** Where a ledger's signing authority came from (proposal 002 section 2). */
export type RootName = "raw" | "identity";

/** What became of an invitation (proposal 002 section 4). */
export type InvitationStatus = "open" | "accepted" | "cancelled";

/** One entry of MembershipView.invitations. */
export interface InvitationEntry {
  invitation_event: string;
  invitation_seq: number;
  invitee: string;
  invitee_key: string;
  role: Role;
  status: InvitationStatus;
}

/** GET /api/identities/:identity_id/memberships. */
export interface MembershipView {
  ok: true;
  ledger_id: string;
  declared_kind: DeclaredKind;
  root: RootName;
  head_seq: number;
  head_event: string;
  principals: PrincipalEntry[];
  invitations: InvitationEntry[];
}

/**
 * POST /api/identities/:identity_id/memberships/invitations. The invitee hands
 * over a descriptor file; the wallet uploads its bytes as base64 and never
 * parses them (contracts/README.md, "Artifacts over JSON").
 */
export interface InviteRequest {
  by: string;
  role: Role;
  invitee_descriptor_base64: string;
}

export interface InvitedResponse {
  ok: true;
  ledger_id: string;
  by: string;
  invitee: string;
  invitee_key: string;
  role: Role;
  invitation_event: string;
  invitation_seq: number;
  timestamp_ms: number;
  head_seq: number;
  head_event: string;
  event: LedgerEvent;
  /** The InvitationBundle to hand the invitee, base64 of the same bytes the CLI writes. */
  invitation_bundle_base64: string;
  event_count: number;
}

/** POST /api/identities/:identity_id/memberships/acceptances. */
export interface AcceptRequest {
  invitation_bundle_base64: string;
}

/**
 * The surface proposal 002 section 4 requires a person to see before anything
 * is signed, plus the file the node signed. The browser holds no keys.
 */
export interface AcceptedResponse {
  ok: true;
  ledger_id: string;
  declared_kind: DeclaredKind;
  root: RootName;
  controllers: PrincipalEntry[];
  invitation_event: string;
  invitee: string;
  invitee_key: string;
  role: Role;
  /** True when accepting means signing as the ledger's own identity. */
  controller_on_raw_root: boolean;
  warning: string | null;
  acceptance_base64: string;
}

/** POST /api/identities/:identity_id/memberships/admissions. */
export interface AdmitRequest {
  by: string;
  acceptance_base64: string;
}

export interface AdmittedResponse {
  ok: true;
  ledger_id: string;
  by: string;
  invitee: string;
  invitee_key: string;
  role: Role;
  invitation_event: string;
  acceptance_event: string;
  acceptance_seq: number;
  timestamp_ms: number;
  head_seq: number;
  head_event: string;
  event: LedgerEvent;
}

/** POST /api/identities/:identity_id/memberships/removals. */
export interface RemoveRequest {
  by: string;
  target: string;
}

export interface RemovedResponse {
  ok: true;
  ledger_id: string;
  by: string;
  target: string;
  principal_removed: boolean;
  /** The open invitation the removal cancelled, null when there was none. */
  invitation_cancelled: string | null;
  removal_event: string;
  removal_seq: number;
  timestamp_ms: number;
  head_seq: number;
  head_event: string;
  event: LedgerEvent;
}

/**
 * The three fields a ProfileUpdate carries (proposal 003 section 1, extended by
 * proposal 005). The payload replaces the whole document, so an omitted field
 * clears that value; both the request body and the before-and-after diff use
 * this shape.
 *
 * email is a claim and nothing more: the node checks its shape (at most 254
 * bytes, one `@` with something on each side) and never its deliverability.
 */
export interface ProfileFields {
  display_name: string | null;
  hostname: string | null;
  email: string | null;
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
  /**
   * The identities the latest witness set names, which are the identities that
   * may keep this ledger (proposal 006 section 1). Endpoint ids used to stand
   * here; an id at this surface is an identity id now.
   */
  witnesses: string[];
  /**
   * The machines this identity's own record says answer for it, in the order it
   * published them (proposal 006 section 2). Empty until it advertises one.
   */
  endpoints: string[];
  /**
   * The machines the retired witness list on its own chain records. Read for
   * resolution and never written again, so no screen draws it.
   */
  witness_endpoints: string[];
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
 * timestamps. display_name and email come from one source, the profile this
 * home holds or the one the last crawl read, so a card shows a known public
 * email without a second request.
 */
export interface ResolvedIdentity {
  identity_id: string;
  display_name: string | null;
  email: string | null;
  alias: string | null;
  hostname: string | null;
  verification_status: VerificationStatus;
  provenance: NameProvenance;
}

/**
 * One row of GET /api/identities/known: an identity this home has a record of
 * and does not control. The first six keys are the ResolvedIdentity fields
 * flattened, without provenance; declared_kind and head_seq come from the
 * stored copy and are null when this home stores none. degrees is the edge
 * count from the nearest crawl root, null when no crawl reached the identity,
 * which means "not in my crawl" and never "no relationship".
 */
export interface KnownIdentity {
  identity_id: string;
  display_name: string | null;
  alias: string | null;
  email: string | null;
  hostname: string | null;
  verification_status: VerificationStatus;
  declared_kind: DeclaredKind | null;
  stored: boolean;
  trusted: boolean;
  degrees: number | null;
  head_seq: number | null;
}

/**
 * GET /api/identities/known?offset=&limit=. Sorted by ascending identity_id
 * alone, and paged since proposal 006 section 8: a home that witnesses holds up
 * to 10000 ledgers, so the default limit is 100 and the maximum is 256.
 */
export interface KnownIdentitiesResponse {
  ok: true;
  offset: number;
  limit: number;
  more: boolean;
  identities: KnownIdentity[];
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

/**
 * One identity this node keeps records for, and whether that identity's own
 * record names this node as a machine that answers for it (proposal 006 section
 * 4.1). reason names what is missing when it does not.
 */
export interface WitnessForEntry {
  identity: string;
  advertised: boolean;
  reason: string | null;
}

/**
 * GET /api/node. One document on every node (proposal 006 section 8): there is
 * no role, and what a node can do is read from what it holds. identity_count is
 * what it can sign for, witness_for is who it accepts records for.
 */
export interface NodeInfo {
  ok: true;
  endpoint_id: string;
  http_bind: string;
  relay: string;
  /** The machines this node pushes to by default, from node.json. */
  witnesses: string[];
  witness_for: WitnessForEntry[];
  storage_capacity: number;
  storage_used: number;
  identity_count: number;
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
 *
 * display_name and email are the proposal 005 addition: when either is given
 * the node appends one ProfileUpdate at seq 1, right after the inception, so a
 * new identity's first two entries are who it is and what it shows the world.
 * Both are omitted when the person left the box empty.
 */
export interface CreateIdentityRequest {
  alias: string;
  declared_kind: DeclaredKind;
  founder?: string;
  display_name?: string;
  email?: string;
}

export interface CreateIdentityResponse {
  ok: true;
  identity: Identity;
  inception_event: string;
}

/**
 * GET /api/identities/:identity_id/keys. Every value is the same lowercase
 * base32 as every other key field in these documents, the two secrets included:
 * on disk the key files hold the same bytes as hex, and this document matches
 * the documents rather than the files (contracts/README.md, "Ids and byte
 * fields"). An identity holding no key of its own answers 409 no_keys_held
 * instead, so a 200 always carries all four values.
 */
export interface IdentityKeysResponse {
  ok: true;
  identity_id: string;
  active_secret_key: string;
  reserve_secret_key: string;
  active_key: string;
  reserve_commit: string;
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

/**
 * POST /api/identities/:identity_id/witnesses. The array names identity ids and
 * replaces the set whole. An id this home cannot resolve to a known identity is
 * refused with reason unresolvable_witness, and one that names a machine this
 * home knows with reason endpoint_not_identity (proposal 006 section 8).
 */
export interface SetWitnessesRequest {
  witnesses: string[];
}

/**
 * POST /api/identities/:identity_id/endpoints. One advertisement naming the
 * machines that answer for this identity, replacing the list whole. An
 * advertisement that would change nothing is refused with reason
 * no_op_endpoint_advertisement.
 */
export interface SetEndpointsRequest {
  endpoints: string[];
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

/** The principal whose key signed an event (proposal 002 section 5). */
export interface SigningPrincipal {
  identity: string;
  key: string;
}

/**
 * Where a machine came from (proposal 006 section 4.2). verified means the
 * identity's own record lists it; hinted means nothing this home holds confirms
 * it. Both are API words and never reach the screen: the UI writes the sentence.
 */
export type Binding = "verified" | "hinted";

/** One machine that answers for an identity, and where that claim came from. */
export interface WitnessEndpoint {
  endpoint_id: string;
  binding: Binding;
}

/**
 * One entry of GET /api/witnesses: a witness is an identity (proposal 006
 * section 1). named_by lists the identities whose folded witness set names it,
 * is_node_default is true for one node.json carries, and stored is true when
 * this home holds a copy of the witness's own record.
 */
export interface WitnessSummary {
  identity_id: string;
  display_name: string | null;
  endpoints: WitnessEndpoint[];
  named_by: string[];
  is_node_default: boolean;
  stored: boolean;
}

export interface WitnessListResponse {
  ok: true;
  witnesses: WitnessSummary[];
}

/** One ledger a witness reports over the sync protocol's List request. */
export interface WitnessLedgerSummary {
  ledger_id: string;
  declared_kind: DeclaredKind;
  head_seq: number;
  head_event: string;
  event_count: number;
  fork_count: number;
}

/**
 * GET /api/witnesses/:identity_id/holdings?offset=&limit=. The node resolves the
 * witness identity to machines and asks one of them; endpoint_id names the one
 * that answered. A witness no machine answers for is 502 witness_unreachable,
 * and an id naming a machine rather than an identity is 404
 * endpoint_not_identity.
 */
export interface WitnessHoldingsResponse {
  ok: true;
  identity_id: string;
  endpoint_id: string;
  ledgers: WitnessLedgerSummary[];
  offset: number;
  limit: number;
  more: boolean;
}

/**
 * What one TXT lookup of _mabel.<hostname>. answered. resolved carries the id;
 * no_record means the name holds no mabel record; mismatched_records means
 * records exist and none parses; unreachable means the resolver did not answer.
 */
export type ResolveStatus = "resolved" | "no_record" | "mismatched_records" | "unreachable";

/** Which of the three things ?input= carried (proposal 006 section 7). */
export type ResolveInputKind = "identity" | "hostname" | "link";

/**
 * GET /api/resolve?input=. Never cached: this is navigation, not verification.
 *
 * status is null on the two kinds that query nothing, an id and a link;
 * endpoints holds the machines a link hinted at, or the mabel-endpoints=
 * records at the label a hostname resolved to.
 */
export interface ResolveResponse {
  ok: true;
  input_kind: ResolveInputKind;
  identity_id: string | null;
  hostname: string | null;
  endpoints: string[];
  status: ResolveStatus | null;
}

/**
 * POST /api/identities/:identity_id/fetch. Two nulls try every source this home
 * knows, in the order of proposal 006 section 5. from names one machine to dial
 * first, from_witness one witness identity to resolve and dial; naming both is
 * refused with reason conflicting_source.
 */
export interface FetchIdentityRequest {
  from: string | null;
  from_witness?: string | null;
}

/**
 * The CLI `sync fetch` document behind a route. stored counts the events this
 * fetch wrote, so 0 means the home was already current; controlled_by names the
 * local identity that may sign for the ledger, null when this home holds none.
 */
export interface FetchIdentityResponse {
  ok: true;
  ledger_id: string;
  source: string;
  event_count: number;
  stored: number;
  head_seq: number;
  head_event: string;
  fetched_at_ms: number;
  controlled_by: string | null;
}

// GET /api/ledgers, GET /api/ledgers/:ledger_id and their events route are gone
// (proposal 006 section 8): GET /api/identities/known, GET
// /api/identities/:identity_id and .../ledger?since= answer for every ledger a
// home holds, whatever else that home does.
