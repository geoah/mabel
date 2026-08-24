import type {
  AcceptRequest,
  AcceptedResponse,
  AddTrustRequest,
  AdmitRequest,
  AdmittedResponse,
  AppendResponse,
  ContactResponse,
  CreateIdentityRequest,
  CreateIdentityResponse,
  ErrorDetails,
  ErrorEnvelope,
  FetchIdentityRequest,
  FetchIdentityResponse,
  ForkListResponse,
  GraphResponse,
  GraphSyncResponse,
  IdentityKeysResponse,
  IdentityListResponse,
  IdentityResponse,
  InviteRequest,
  InvitedResponse,
  LedgerEntryResponse,
  LedgerListResponse,
  LedgerPageResponse,
  LookupResponse,
  MembershipView,
  RemoveRequest,
  RemovedResponse,
  ReplaceProfileRequest,
  ReplaceProfileResponse,
  ResolveResponse,
  RevokeTrustRequest,
  RevokeTrustResponse,
  SetContactRequest,
  SetWitnessesRequest,
  SyncPushRequest,
  SyncPushResponse,
  VerificationResponse,
  WalletNodeInfo,
  WitnessLedgerListResponse,
  WitnessListResponse,
  WitnessNodeInfo,
} from "./types";

/** The error envelope of contracts/README.md, thrown for any non-ok document. */
export class ApiError extends Error {
  readonly code: number;
  readonly details: ErrorDetails;
  readonly status: number;

  constructor(envelope: ErrorEnvelope, status: number) {
    super(envelope.message);
    this.name = "ApiError";
    this.code = envelope.code;
    this.details = envelope.details;
    this.status = status;
  }

  get reason(): string {
    return this.details.reason;
  }
}

export const API_BASE = "/api";

function isErrorEnvelope(body: unknown): body is ErrorEnvelope {
  return typeof body === "object" && body !== null && (body as { ok?: unknown }).ok === false;
}

/**
 * The node serves the UI and the API from the same loopback origin, so requests
 * stay same-origin. The origin is spelled out because fetch outside a browser
 * does not resolve a relative path.
 */
function url(path: string): string {
  return new URL(`${API_BASE}${path}`, globalThis.location.origin).toString();
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url(path), init);
  let body: unknown;
  try {
    body = await response.json();
  } catch {
    throw new ApiError(
      {
        ok: false,
        code: 2,
        message: `${response.status} ${response.statusText}: response was not JSON`,
        details: { reason: "malformed_response", status: response.status },
      },
      response.status,
    );
  }
  if (isErrorEnvelope(body)) {
    throw new ApiError(body, response.status);
  }
  if (!response.ok) {
    throw new ApiError(
      {
        ok: false,
        code: 2,
        message: `${response.status} ${response.statusText}`,
        details: { reason: "unexpected_status", status: response.status },
      },
      response.status,
    );
  }
  return body as T;
}

function get<T>(path: string): Promise<T> {
  return request<T>(path);
}

/**
 * Mutating routes must send content-type: application/json, and the node checks
 * Origin against its own host (proposal 001 section 10).
 */
function post<T>(path: string, body: unknown): Promise<T> {
  return request<T>(path, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

/** The contact document is the one route replaced whole under its own verb. */
function put<T>(path: string, body: unknown): Promise<T> {
  return request<T>(path, {
    method: "PUT",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(body),
  });
}

function query(params: Record<string, string | number | undefined>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined) {
      search.set(key, String(value));
    }
  }
  const rendered = search.toString();
  return rendered ? `?${rendered}` : "";
}

// Wallet routes.

/** A node has one role, and the same bundle is served by both: the shell asks. */
export function getNode(): Promise<WalletNodeInfo | WitnessNodeInfo> {
  return get<WalletNodeInfo | WitnessNodeInfo>("/node");
}

export function listIdentities(): Promise<IdentityListResponse> {
  return get<IdentityListResponse>("/identities");
}

export function createIdentity(body: CreateIdentityRequest): Promise<CreateIdentityResponse> {
  return post<CreateIdentityResponse>("/identities", body);
}

export function getIdentity(identityId: string): Promise<IdentityResponse> {
  return get<IdentityResponse>(`/identities/${identityId}`);
}

/** The two secret keys of one identity, for a person to save (decision 017). */
export function getIdentityKeys(identityId: string): Promise<IdentityKeysResponse> {
  return get<IdentityKeysResponse>(`/identities/${identityId}/keys`);
}

/** since is inclusive: the first event returned has seq === since. */
export function getIdentityLedger(
  identityId: string,
  params: { since?: number; limit?: number } = {},
): Promise<LedgerPageResponse> {
  return get<LedgerPageResponse>(`/identities/${identityId}/ledger${query(params)}`);
}

export function setIdentityWitnesses(
  identityId: string,
  body: SetWitnessesRequest,
): Promise<AppendResponse> {
  return post<AppendResponse>(`/identities/${identityId}/witnesses`, body);
}

export function addTrust(body: AddTrustRequest): Promise<AppendResponse> {
  return post<AppendResponse>("/trust", body);
}

export function revokeTrust(
  eventId: string,
  body: RevokeTrustRequest,
): Promise<RevokeTrustResponse> {
  return post<RevokeTrustResponse>(`/trust/${eventId}/revoke`, body);
}

export function syncPush(body: SyncPushRequest): Promise<SyncPushResponse> {
  return post<SyncPushResponse>("/sync/push", body);
}

/**
 * The CLI `sync fetch` behind a route: pulls a ledger this home does not hold.
 * A null `from` tries the known witnesses in the crawler's source order.
 */
export function fetchIdentity(
  identityId: string,
  body: FetchIdentityRequest = { from: null },
): Promise<FetchIdentityResponse> {
  return post<FetchIdentityResponse>(`/identities/${identityId}/fetch`, body);
}

/**
 * Replacement, not patch: both keys are always sent and a null clears that
 * name. A replacement that would change nothing answers 409
 * no_op_profile_update (proposal 003 section 1).
 */
export function replaceProfile(
  identityId: string,
  body: ReplaceProfileRequest,
): Promise<ReplaceProfileResponse> {
  return post<ReplaceProfileResponse>(`/identities/${identityId}/profile`, body);
}

/** Forces a DNS check and waits for it; the GET routes answer from cache. */
export function forceVerification(identityId: string): Promise<VerificationResponse> {
  return post<VerificationResponse>(`/identities/${identityId}/verification`, {});
}

/** The local contact store, valid for foreign identity ids too. */
export function getContact(identityId: string): Promise<ContactResponse> {
  return get<ContactResponse>(`/identities/${identityId}/contact`);
}

export function setContact(
  identityId: string,
  body: SetContactRequest,
): Promise<ContactResponse> {
  return put<ContactResponse>(`/identities/${identityId}/contact`, body);
}

// The membership routes (ticket 021). Every artifact crosses as base64 of the
// same bytes the CLI writes, and the node does the signing: the browser holds
// no keys (proposal 001 section 10).

export function getMemberships(identityId: string): Promise<MembershipView> {
  return get<MembershipView>(`/identities/${identityId}/memberships`);
}

export function invite(identityId: string, body: InviteRequest): Promise<InvitedResponse> {
  return post<InvitedResponse>(`/identities/${identityId}/memberships/invitations`, body);
}

/**
 * Called on the invitee's own ledger: the node signs the acceptance and answers
 * with the surface it signed under, so a person sees the ledger, its
 * controllers and the raw-root warning before the file leaves the wallet.
 */
export function acceptInvitation(
  identityId: string,
  body: AcceptRequest,
): Promise<AcceptedResponse> {
  return post<AcceptedResponse>(`/identities/${identityId}/memberships/acceptances`, body);
}

export function admit(identityId: string, body: AdmitRequest): Promise<AdmittedResponse> {
  return post<AdmittedResponse>(`/identities/${identityId}/memberships/admissions`, body);
}

export function removePrincipal(
  identityId: string,
  body: RemoveRequest,
): Promise<RemovedResponse> {
  return post<RemovedResponse>(`/identities/${identityId}/memberships/removals`, body);
}

/**
 * "How do I know this identity", relative to one local root. from defaults on
 * the node to the lowest local identity id, so the wallet sends the identity
 * the selector holds.
 */
export function lookup(identityId: string, params: { from?: string } = {}): Promise<LookupResponse> {
  return get<LookupResponse>(`/lookup/${identityId}${query(params)}`);
}

/** The current crawl generation, null when no crawl has run in this home. */
export function getGraph(): Promise<GraphResponse> {
  return get<GraphResponse>("/graph");
}

/** One crawl, run now: synchronizing is manual, there is no background timer. */
export function syncGraph(): Promise<GraphSyncResponse> {
  return post<GraphSyncResponse>("/graph/sync", {});
}

/**
 * Every witness this wallet knows of: the folded witness configs of its stored
 * ledgers plus the defaults node.json carries.
 */
export function listWitnesses(): Promise<WitnessListResponse> {
  return get<WitnessListResponse>("/witnesses");
}

/**
 * What one witness holds, asked live over the sync protocol's List request. A
 * witness this node cannot reach answers 502 with reason witness_unreachable.
 */
export function listWitnessLedgers(
  endpointId: string,
  params: { offset?: number; limit?: number } = {},
): Promise<WitnessLedgerListResponse> {
  return get<WitnessLedgerListResponse>(`/witnesses/${endpointId}/ledgers${query(params)}`);
}

/**
 * One TXT lookup of _mabel.<hostname>., for navigation. It is never cached and
 * it verifies nothing: a resolved id still renders its own advisory verdict.
 */
export function resolveHostname(hostname: string): Promise<ResolveResponse> {
  return get<ResolveResponse>(`/resolve/${encodeURIComponent(hostname)}`);
}

// Witness routes, read-only.

export function listLedgers(
  params: { offset?: number; limit?: number } = {},
): Promise<LedgerListResponse> {
  return get<LedgerListResponse>(`/ledgers${query(params)}`);
}

export function getLedger(ledgerId: string): Promise<LedgerEntryResponse> {
  return get<LedgerEntryResponse>(`/ledgers/${ledgerId}`);
}

export function getLedgerEvents(
  ledgerId: string,
  params: { since?: number; limit?: number } = {},
): Promise<LedgerPageResponse> {
  return get<LedgerPageResponse>(`/ledgers/${ledgerId}/events${query(params)}`);
}

export function listForks(
  params: { ledger_id?: string; offset?: number; limit?: number } = {},
): Promise<ForkListResponse> {
  return get<ForkListResponse>(`/forks${query(params)}`);
}
