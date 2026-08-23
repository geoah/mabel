import type {
  AddTrustRequest,
  AppendResponse,
  CreateIdentityRequest,
  CreateIdentityResponse,
  ErrorDetails,
  ErrorEnvelope,
  ForkListResponse,
  IdentityListResponse,
  IdentityResponse,
  LedgerEntryResponse,
  LedgerListResponse,
  LedgerPageResponse,
  RevokeTrustRequest,
  RevokeTrustResponse,
  SetWitnessesRequest,
  SyncPushRequest,
  SyncPushResponse,
  VerifyLedgerReport,
  VerifyLedgerRequest,
  VerifyTrustReport,
  VerifyTrustRequest,
  WalletNodeInfo,
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

export function getWalletNode(): Promise<WalletNodeInfo> {
  return get<WalletNodeInfo>("/node");
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

export function verifyTrust(body: VerifyTrustRequest): Promise<VerifyTrustReport> {
  return post<VerifyTrustReport>("/verify", body);
}

export function verifyLedger(body: VerifyLedgerRequest): Promise<VerifyLedgerReport> {
  return post<VerifyLedgerReport>("/verify", body);
}

// Witness routes, read-only.

export function getWitnessNode(): Promise<WitnessNodeInfo> {
  return get<WitnessNodeInfo>("/node");
}

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
