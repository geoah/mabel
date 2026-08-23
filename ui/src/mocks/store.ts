// An in-memory node, seeded from the frozen fixtures. It exists so `npm run dev`
// and the component tests drive the same responses without a backend. It is not
// a model of the node: no keys, no crypto, no chain verification.

import type {
  AddTrustRequest,
  AppendResponse,
  CreateIdentityRequest,
  CreateIdentityResponse,
  ErrorEnvelope,
  ForkListResponse,
  ForkRecord,
  Identity,
  IdentityListResponse,
  IdentityResponse,
  LedgerEntryResponse,
  LedgerEvent,
  LedgerListResponse,
  LedgerPageResponse,
  RevokeTrustRequest,
  RevokeTrustResponse,
  SyncPushRequest,
  SyncPushResponse,
  VerifyLedgerReport,
  VerifyLedgerRequest,
  VerifyTrustReport,
  VerifyTrustRequest,
  WalletNodeInfo,
  WitnessNodeInfo,
} from "@/api/types";
import type { HeldLedger } from "./fixtures";
import {
  ALICE,
  BOB,
  createdIdentity,
  errors,
  seedEvents,
  seedIdentities,
  syncPush as syncPushFixture,
  verifyLedgerValid,
  verifyTrustRevoked,
  verifyTrustTrusted,
  verifyTrustUnresolved,
  walletNode,
  witnessForks,
  witnessLedgers,
  witnessNode,
} from "./fixtures";

/** A rejected request: the HTTP status plus the error envelope body. */
export class MockFailure extends Error {
  constructor(
    readonly status: number,
    readonly body: ErrorEnvelope,
  ) {
    super(body.message);
    this.name = "MockFailure";
  }
}

function fail(named: { status: number; body: ErrorEnvelope }): never {
  throw new MockFailure(named.status, named.body);
}

function failWith(status: number, body: ErrorEnvelope): never {
  throw new MockFailure(status, body);
}

const BASE32 = "abcdefghijklmnopqrstuvwxyz234567";
let minted = 0;

/** A 52-character lowercase base32 id, shaped like a real one and nothing more. */
function mintId(seed: string): string {
  minted += 1;
  let hash = (2166136261 ^ minted) >>> 0;
  for (const character of seed) {
    hash = Math.imul(hash ^ character.charCodeAt(0), 16777619) >>> 0;
  }
  let out = "";
  for (let index = 0; index < 52; index += 1) {
    hash = Math.imul(hash ^ (index + 1), 16777619) >>> 0;
    out += BASE32[(hash >>> 7) % 32];
  }
  return out;
}

export type NodeRole = "wallet" | "witness";

/**
 * A real node has exactly one role, but the mock serves both routes from one
 * process. In the browser the role follows the path being browsed, so /witness
 * sees a witness document; a test driving a memory router sets it explicitly.
 */
let nodeRole: NodeRole | null = null;

export function setNodeRole(role: NodeRole | null): void {
  nodeRole = role;
}

interface State {
  identities: Identity[];
  events: Map<string, LedgerEvent[]>;
  /** The witness side, independent of the wallet: a witness holds copies. */
  held: HeldLedger[];
  forks: ForkRecord[];
}

let state: State = emptyState();

function emptyState(): State {
  return { identities: [], events: new Map(), held: [], forks: [] };
}

export function resetStore(): void {
  minted = 0;
  nodeRole = null;
  state = {
    identities: seedIdentities.map((identity) => ({
      ...identity,
      witnesses: [...identity.witnesses],
      trust: identity.trust.map((record) => ({ ...record })),
    })),
    events: new Map([[ALICE, seedEvents()]]),
    held: witnessLedgers(),
    forks: witnessForks(),
  };
}

resetStore();

function find(identityId: string): Identity {
  const identity = state.identities.find((entry) => entry.identity_id === identityId);
  if (!identity) {
    failWith(404, {
      ok: false,
      code: 2,
      message: `no identity ${identityId} in this node home`,
      details: { reason: "identity_not_found", identity_id: identityId },
    });
  }
  return identity;
}

function chain(identityId: string): LedgerEvent[] {
  const events = state.events.get(identityId);
  if (!events) {
    const created: LedgerEvent[] = [];
    state.events.set(identityId, created);
    return created;
  }
  return events;
}

function append(
  identity: Identity,
  payloadKind: string,
  payload: Record<string, unknown>,
): LedgerEvent {
  const events = chain(identity.identity_id);
  const event: LedgerEvent = {
    event_id: mintId(`${identity.identity_id}:${payloadKind}:${events.length}`),
    seq: identity.head_seq + 1,
    ledger_id: identity.identity_id,
    prev: identity.head_event,
    timestamp_ms: Date.now(),
    author_key: identity.active_key ?? identity.principals[0].active_key,
    payload_kind: payloadKind,
    payload,
  };
  events.push(event);
  identity.head_seq = event.seq;
  identity.head_event = event.event_id;
  identity.event_count = events.length;
  return event;
}

function appendResponse(identity: Identity, event: LedgerEvent): AppendResponse {
  return {
    ok: true,
    ledger_id: identity.identity_id,
    head_seq: identity.head_seq,
    head_event: identity.head_event,
    event,
  };
}

export function listIdentities(): IdentityListResponse {
  const identities = [...state.identities].sort((left, right) =>
    left.identity_id < right.identity_id ? -1 : 1,
  );
  return { ok: true, identities };
}

export function getIdentity(identityId: string): IdentityResponse {
  return { ok: true, identity: find(identityId) };
}

export function createIdentity(body: Partial<CreateIdentityRequest>): CreateIdentityResponse {
  if (!body.alias) {
    fail(errors.missingField);
  }
  const kinds = ["person", "organization", "agent", "service"];
  if (!body.declared_kind || !kinds.includes(body.declared_kind)) {
    failWith(400, {
      ok: false,
      code: 10,
      message: "Schema error: declared_kind must be one of person, organization, agent, service",
      details: {
        reason: "unknown_enum_value",
        field: "declared_kind",
        value: String(body.declared_kind),
      },
    });
  }
  if (body.founder) {
    // An identity root names one founding principal that must be a local
    // identity with a raw root (proposal 002 section 2).
    find(body.founder);
  }
  const identityId = mintId(body.alias);
  const rawRoot = !body.founder;
  const activeKey = String(createdIdentity.identity.active_key);
  const founder = rawRoot ? identityId : String(body.founder);
  // A raw root signs with its own new key; an identity root signs with the
  // founder's, which this home already holds.
  const founderKey = rawRoot ? activeKey : String(find(founder).active_key);
  const identity: Identity = {
    identity_id: identityId,
    declared_kind: body.declared_kind,
    alias: body.alias,
    created_at_ms: Date.now(),
    head_seq: 0,
    head_event: identityId,
    event_count: 1,
    witnesses: [],
    trust: [],
    // The inception seeds exactly one principal: the ledger itself on a raw
    // root, the founding identity on an identity root (proposal 002 section 1).
    principals: [
      { identity: founder, active_key: founderKey, role: "controller", is_root: true },
    ],
    open_invitation_count: 0,
    ...(rawRoot
      ? {
          active_key: activeKey,
          reserve_commit: createdIdentity.identity.reserve_commit,
        }
      : {}),
  };
  state.identities.push(identity);
  // One inception payload_kind for both roots, with the root oneof under `root`
  // (contracts/README.md, "Event document").
  state.events.set(identityId, [
    {
      event_id: identityId,
      seq: 0,
      ledger_id: null,
      prev: null,
      timestamp_ms: identity.created_at_ms,
      author_key: founderKey,
      payload_kind: "inception",
      payload: {
        declared_kind: identity.declared_kind,
        nonce: "ugq2dinbugq2dinbugq2dinbue",
        root: rawRoot
          ? {
              raw_root: {
                active_key: identity.active_key,
                reserve_commit: identity.reserve_commit,
              },
            }
          : {
              identity_root: {
                founder,
                founder_key: founderKey,
                // The founder's inception event, embedded verbatim and rendered
                // as base32 of those bytes, not as a decoded message.
                founder_inception: `${mintId(founder)}${mintId(founder)}`.slice(0, 63),
              },
            },
      },
    },
  ]);
  return { ok: true, identity, inception_event: identityId };
}

export function getIdentityLedger(
  identityId: string,
  params: { since?: number; limit?: number },
): LedgerPageResponse {
  const identity = find(identityId);
  const since = params.since ?? 0;
  const limit = params.limit ?? 512;
  if (!Number.isInteger(since) || since < 0) {
    failWith(400, {
      ok: false,
      code: 2,
      message: "since must be a non-negative integer",
      details: {
        reason: "malformed_query_parameter",
        parameter: "since",
        value: String(params.since),
      },
    });
  }
  const events = chain(identityId);
  // ?since= is inclusive: the page starts at seq === since.
  const matching = events.filter((event) => event.seq >= since);
  return {
    ok: true,
    ledger_id: identityId,
    declared_kind: identity.declared_kind,
    since,
    limit,
    head_seq: identity.head_seq,
    head_event: identity.head_event,
    event_count: events.length,
    more: matching.length > limit,
    events: matching.slice(0, limit),
  };
}

export function setIdentityWitnesses(identityId: string, witnesses: string[]): AppendResponse {
  const identity = find(identityId);
  const duplicate = witnesses.find(
    (endpoint, index) => witnesses.indexOf(endpoint) !== index,
  );
  if (witnesses.length < 1 || witnesses.length > 16 || duplicate) {
    failWith(400, {
      ok: false,
      code: 10,
      message: "Schema error: witnesses must hold 1 to 16 distinct endpoint ids",
      details: {
        reason: duplicate ? "duplicate_witness" : "witness_count_out_of_range",
        field: "witnesses",
        value: duplicate ?? String(witnesses.length),
      },
    });
  }
  identity.witnesses = [...witnesses];
  const event = append(identity, "witness_config", { witnesses: [...witnesses] });
  return appendResponse(identity, event);
}

export function addTrust(body: Partial<AddTrustRequest>): AppendResponse {
  const identity = find(String(body.issuer));
  const subject = String(body.subject);
  if (subject === identity.identity_id) {
    failWith(400, {
      ok: false,
      code: 10,
      message: "Schema error: subject must differ from the issuer ledger id",
      details: { reason: "subject_equals_ledger", field: "subject", value: subject },
    });
  }
  const open = identity.trust.find((record) => record.subject === subject && !record.revoked);
  if (open) {
    failWith(409, {
      ok: false,
      code: 20,
      message: `Policy error: an unrevoked attestation for ${subject} already exists at seq ${open.attestation_seq}`,
      details: {
        reason: "duplicate_unrevoked_attestation",
        subject,
        attestation_event: open.attestation_event,
        at_seq: open.attestation_seq,
      },
    });
  }
  const event = append(identity, "trust_attestation", { subject });
  identity.trust.push({
    attestation_event: event.event_id,
    attestation_seq: event.seq,
    subject,
    revoked: false,
    revocation_event: null,
    revocation_seq: null,
  });
  return appendResponse(identity, event);
}

export function revokeTrust(
  eventId: string,
  body: Partial<RevokeTrustRequest>,
): RevokeTrustResponse {
  const identity = find(String(body.issuer));
  const record = identity.trust.find((entry) => entry.attestation_event === eventId);
  if (!record) {
    failWith(404, {
      ok: false,
      code: 20,
      message: `Policy error: ${eventId} names no attestation in ledger ${identity.identity_id}`,
      details: {
        reason: "unknown_attestation",
        ledger_id: identity.identity_id,
        event_id: eventId,
      },
    });
  }
  if (record.revoked) {
    failWith(409, {
      ok: false,
      code: 20,
      message: `Policy error: attestation ${eventId} is already revoked at seq ${record.revocation_seq}`,
      details: {
        reason: "attestation_already_revoked",
        attestation_event: eventId,
        revocation_event: record.revocation_event,
        at_seq: record.revocation_seq,
      },
    });
  }
  const event = append(identity, "trust_revocation", { target: eventId });
  record.revoked = true;
  record.revocation_event = event.event_id;
  record.revocation_seq = event.seq;
  return {
    ok: true,
    ledger_id: identity.identity_id,
    head_seq: identity.head_seq,
    head_event: identity.head_event,
    revoked_attestation: record.attestation_event,
    revoked_attestation_seq: record.attestation_seq,
    event,
  };
}

export function syncPush(body: Partial<SyncPushRequest>): SyncPushResponse {
  const identity = find(String(body.identity_id));
  const targets = body.to ? [body.to] : identity.witnesses;
  if (targets.length === 0) {
    failWith(errors.allWitnessesFailed.status, errors.allWitnessesFailed.body);
  }
  const unreachable = syncPushFixture.results[1];
  return {
    ok: true,
    ledger_id: identity.identity_id,
    head_seq: identity.head_seq,
    head_event: identity.head_event,
    results: targets.map((endpoint, index) =>
      // The second configured witness stands in for the unreachable case, so the
      // per-witness table has both outcomes to render.
      index === 1
        ? { ...unreachable, endpoint, message: `Network error: no route to ${endpoint} after 10s` }
        : {
            endpoint,
            status: "accepted",
            head_seq: identity.head_seq,
            stored: identity.event_count,
            reject_code: null,
            at_seq: null,
            message: null,
          },
    ),
  };
}

function knownLedger(ledgerId: string): boolean {
  return ledgerId === BOB || state.identities.some((entry) => entry.identity_id === ledgerId);
}

export function verifyTrust(body: VerifyTrustRequest): VerifyTrustReport {
  const identity = find(body.issuer);
  const records = identity.trust.filter((record) => record.subject === body.subject);
  const named = { issuer: body.issuer, subject: body.subject };
  if (!knownLedger(body.subject)) {
    return { ...verifyTrustUnresolved, ...named };
  }
  if (records.some((record) => !record.revoked)) {
    return { ...verifyTrustTrusted, ...named };
  }
  if (records.length > 0) {
    return { ...verifyTrustRevoked, ...named };
  }
  // No attestation was ever issued, so nothing was revoked either.
  return {
    ...verifyTrustTrusted,
    ...named,
    trusted: false,
    attestation_event: null,
    attestation_seq: null,
  };
}

export function verifyLedger(body: VerifyLedgerRequest): VerifyLedgerReport {
  const identity = find(body.ledger_id);
  return {
    ...verifyLedgerValid,
    ledger_id: identity.identity_id,
    declared_kind: identity.declared_kind,
    event_count: identity.event_count,
  };
}

// The witness routes. Every one of them reads: a witness serves no mutation
// over HTTP (proposal 001 section 10).

export function nodeInfo(): WalletNodeInfo | WitnessNodeInfo {
  const browsed = globalThis.location?.pathname.startsWith("/witness") ? "witness" : "wallet";
  if ((nodeRole ?? browsed) !== "witness") {
    return walletNode;
  }
  // The counts follow the seeded store, not the fixture, so the node document
  // and the ledger list agree.
  return {
    ...witnessNode,
    ledger_count: state.held.length,
    fork_count: state.forks.length,
  };
}

function checkLedgerId(ledgerId: string): void {
  if (ledgerId.length !== 52) {
    failWith(400, {
      ok: false,
      code: 10,
      message: "Schema error: ledger id must be 52 base32 characters",
      details: { reason: "malformed_ledger_id", value: ledgerId },
    });
  }
}

function checkRange(name: string, value: number | undefined): number | undefined {
  if (value !== undefined && (!Number.isInteger(value) || value < 0)) {
    failWith(400, {
      ok: false,
      code: 2,
      message: `${name} must be a non-negative integer`,
      details: { reason: "malformed_query_parameter", parameter: name, value: String(value) },
    });
  }
  return value;
}

function held(ledgerId: string): HeldLedger {
  checkLedgerId(ledgerId);
  const found = state.held.find((entry) => entry.entry.ledger_id === ledgerId);
  if (!found) {
    failWith(404, {
      ok: false,
      code: 2,
      message: `this witness does not hold ${ledgerId}`,
      details: { reason: "ledger_not_held", ledger_id: ledgerId },
    });
  }
  return found;
}

/** GET /api/ledgers, ordered by ascending ledger_id. */
export function listLedgers(params: { offset?: number; limit?: number }): LedgerListResponse {
  const offset = checkRange("offset", params.offset) ?? 0;
  const limit = checkRange("limit", params.limit) ?? 256;
  const ordered = [...state.held].sort((left, right) =>
    left.entry.ledger_id < right.entry.ledger_id ? -1 : 1,
  );
  return {
    ok: true,
    offset,
    limit,
    more: ordered.length > offset + limit,
    entries: ordered.slice(offset, offset + limit).map((entry) => entry.entry),
  };
}

export function getLedgerEntry(ledgerId: string): LedgerEntryResponse {
  const found = held(ledgerId);
  return { ok: true, entry: found.entry, witnesses: [...found.witnesses] };
}

/** GET /api/ledgers/:ledger_id/events. since is inclusive: the page opens at seq === since. */
export function getLedgerEvents(
  ledgerId: string,
  params: { since?: number; limit?: number },
): LedgerPageResponse {
  const found = held(ledgerId);
  const since = checkRange("since", params.since) ?? 0;
  const limit = checkRange("limit", params.limit) ?? 512;
  const matching = found.events.filter((event) => event.seq >= since);
  return {
    ok: true,
    ledger_id: found.entry.ledger_id,
    declared_kind: found.entry.declared_kind,
    since,
    limit,
    head_seq: found.entry.head_seq,
    head_event: found.entry.head_event,
    event_count: found.entry.event_count,
    more: matching.length > limit,
    events: matching.slice(0, limit),
  };
}

/** GET /api/forks, ordered by ledger_id then seq, optionally filtered to one ledger. */
export function listForks(params: {
  ledger_id?: string;
  offset?: number;
  limit?: number;
}): ForkListResponse {
  const offset = checkRange("offset", params.offset) ?? 0;
  const limit = checkRange("limit", params.limit) ?? 64;
  if (params.ledger_id !== undefined) {
    checkLedgerId(params.ledger_id);
  }
  const matching = state.forks
    .filter((record) => params.ledger_id === undefined || record.ledger_id === params.ledger_id)
    .sort((left, right) =>
      left.ledger_id === right.ledger_id
        ? left.seq - right.seq
        : left.ledger_id < right.ledger_id
          ? -1
          : 1,
    );
  return {
    ok: true,
    offset,
    limit,
    more: matching.length > offset + limit,
    entries: matching.slice(offset, offset + limit),
  };
}
