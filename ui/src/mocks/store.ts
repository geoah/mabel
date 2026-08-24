// An in-memory node, seeded from the frozen fixtures. It exists so `npm run dev`
// and the component tests drive the same responses without a backend. It is not
// a model of the node: no keys, no crypto, no chain verification.

import type {
  AcceptRequest,
  AcceptedResponse,
  AddTrustRequest,
  AdmitRequest,
  AdmittedResponse,
  AppendResponse,
  Contact,
  ContactResponse,
  CreateIdentityRequest,
  CreateIdentityResponse,
  DeclaredKind,
  ErrorEnvelope,
  FetchIdentityRequest,
  FetchIdentityResponse,
  Graph,
  GraphResponse,
  GraphSyncResponse,
  Identity,
  IdentityKeysResponse,
  IdentityListResponse,
  IdentityResponse,
  InvitationEntry,
  InviteRequest,
  InvitedResponse,
  KnownIdentitiesResponse,
  KnownIdentity,
  LedgerEvent,
  LedgerPageResponse,
  LookupHop,
  LookupResponse,
  MembershipView,
  NodeInfo,
  PrincipalEntry,
  ProfileFields,
  RemoveRequest,
  RemovedResponse,
  ReplaceProfileResponse,
  ResolveResponse,
  ResolveStatus,
  ResolvedIdentity,
  Role,
  RootName,
  RevokeTrustRequest,
  RevokeTrustResponse,
  SetContactRequest,
  SyncPushRequest,
  SyncPushResponse,
  Verification,
  VerificationResponse,
  WitnessEndpoint,
  WitnessHoldingsResponse,
  WitnessListResponse,
  WitnessSummary,
} from "@/api/types";
import type { HeldLedger } from "./fixtures";
import {
  ACME,
  ALICE,
  BOB,
  HINTED_MACHINE,
  REACHABLE_WITNESS,
  UNREACHABLE_MACHINE,
  UNREACHABLE_WITNESS,
  WITNESS_MACHINE,
  WITNESS_NAME,
  acmeEvents,
  aliceEvents,
  createdIdentity,
  errors,
  identityKeys,
  knownBob,
  endpointNotWitnessError,
  knownWitnesses,
  nodeDocument,
  noKeysHeldError,
  noOpEndpointsError,
  seedContact,
  seedEdges,
  seedGraph,
  seedIdentities,
  seedLookup,
  seedResolved,
  syncPush as syncPushFixture,
  unresolvableWitnessError,
  witnessEvents,
  witnessLedgers,
} from "./fixtures";
import { MOCK_STATE_KEY } from "./persistence";

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

/** status unclaimed with every other key null: the profile names no hostname. */
const UNCLAIMED: Verification = {
  hostname: null,
  status: "unclaimed",
  checked_at_ms: null,
  last_verified_at_ms: null,
  stale: false,
  detail: null,
  unreachable: null,
};

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

/** One crawled attestation: who attests, to whom, with which event. */
interface Edge {
  from: string;
  to: string;
  attestation_event: string;
  seq: number;
}

interface State {
  identities: Identity[];
  /**
   * Ledgers this home stored without controlling them, the result of a fetch.
   * GET /api/identities/:id answers for them; GET /api/identities does not list
   * them, because that list is what the wallet can sign for.
   */
  fetched: Map<string, Identity>;
  events: Map<string, LedgerEvent[]>;
  /** Every invitation a ledger issued, by ledger id, cancelled ones included. */
  invitations: Map<string, InvitationEntry[]>;
  /** contacts/<identity_id>.json, foreign ids included, never signed or synced. */
  contacts: Map<string, Contact>;
  /** The crawl generation this home holds, null before the first sync. */
  graph: Graph | null;
  /** How the crawl named the identities it reached, keyed by identity id. */
  resolved: Map<string, ResolvedIdentity>;
  edges: Edge[];
  /** What another witness serves over the sync protocol, for the holdings proxy. */
  held: HeldLedger[];
}

let state: State = emptyState();

function emptyState(): State {
  return {
    identities: [],
    fetched: new Map(),
    events: new Map(),
    invitations: new Map(),
    contacts: new Map(),
    graph: null,
    resolved: new Map(),
    edges: [],
    held: [],
  };
}

/**
 * The state a reload has to keep, as plain JSON: everything a visitor changed.
 * The witness side is left out because no route mutates it, so it is reseeded
 * from the fixtures every boot.
 */
interface Snapshot {
  version: string;
  minted: number;
  identities: Identity[];
  fetched: [string, Identity][];
  events: [string, LedgerEvent[]][];
  invitations: [string, InvitationEntry[]][];
  contacts: [string, Contact][];
  graph: Graph | null;
  resolved: [string, ResolvedIdentity][];
  edges: Edge[];
}

/**
 * The saved state is thrown away when this string changes. The leading number is
 * bumped by hand when the shape above or the seed below changes; the rest is the
 * version the node fixture reports, which moves when the fixtures do.
 */
const SNAPSHOT_VERSION = `3:${nodeDocument.version}`;

function snapshot(): Snapshot {
  return {
    version: SNAPSHOT_VERSION,
    minted,
    identities: state.identities,
    fetched: [...state.fetched],
    events: [...state.events],
    invitations: [...state.invitations],
    contacts: [...state.contacts],
    graph: state.graph,
    resolved: [...state.resolved],
    edges: state.edges,
  };
}

/**
 * Writes what the visitor did. A harness whose fetched record disappears on the
 * next page load is telling a lie about the node, so every mutating route ends
 * here. A storage that throws (private mode, disabled cookies) means the session
 * still works and nothing is remembered.
 */
export function persistStore(): void {
  try {
    globalThis.localStorage?.setItem(MOCK_STATE_KEY, JSON.stringify(snapshot()));
  } catch {
    // Nothing is remembered, and the session still works.
  }
}

/** What the last page load saved, or null when there is nothing usable. */
function savedSnapshot(): Snapshot | null {
  try {
    const raw = globalThis.localStorage?.getItem(MOCK_STATE_KEY) ?? null;
    if (raw === null) {
      return null;
    }
    const parsed = JSON.parse(raw) as Partial<Snapshot>;
    // A snapshot from another version is thrown away rather than migrated: it
    // was seeded from fixtures this build no longer carries.
    return parsed.version === SNAPSHOT_VERSION ? (parsed as Snapshot) : null;
  } catch {
    return null;
  }
}

/**
 * Loads what the last page load left, or reseeds from the fixtures when there is
 * nothing saved, the saved state was written by another version, or it does not
 * parse. Called once on boot and by the tests that drive a reload.
 */
export function restoreStore(): boolean {
  const saved = savedSnapshot();
  if (saved === null) {
    resetStore();
    return false;
  }
  minted = saved.minted;
  state = {
    identities: saved.identities,
    fetched: new Map(saved.fetched),
    events: new Map(saved.events),
    invitations: new Map(saved.invitations),
    contacts: new Map(saved.contacts),
    graph: saved.graph,
    resolved: new Map(saved.resolved),
    edges: saved.edges,
    // What another witness serves is a read-only fixture, never saved.
    held: witnessLedgers(),
  };
  return true;
}

export function resetStore(): void {
  minted = 0;
  const contacts = new Map<string, Contact>();
  // Bob is a foreign identity with a local note: the contact store covers ids
  // this home does not control.
  contacts.set(BOB, { ...seedContact });
  const identities = seedIdentities.map((identity) => ({
    ...identity,
    // A witness set names identities (proposal 006 section 1), which is what
    // the frozen list carries. Acme names the second one and nothing else, so
    // the witness list has a row that is not a node default.
    witnesses:
      identity.identity_id === ACME ? [UNREACHABLE_WITNESS] : [...identity.witnesses],
    endpoints: [...identity.endpoints],
    witness_endpoints: [...identity.witness_endpoints],
    trust: identity.trust.map((record) => ({ ...record })),
    profile: identity.profile ? { ...identity.profile } : null,
    verification: { ...identity.verification },
    contact: identity.contact ? { ...identity.contact } : null,
  }));
  for (const identity of identities) {
    if (identity.contact) {
      contacts.set(identity.identity_id, { ...identity.contact });
    }
  }
  const held = witnessLedgers();
  const bob = storedBob(held);
  const witness = storedWitness();
  state = {
    identities,
    // Two records this home stored and controls nothing about: Bob, the known
    // identity with a copy on disk, and the witness Alice named, whose own
    // record is what says which machine answers for it.
    fetched: new Map([
      ...(bob === null ? [] : ([[BOB, bob.identity]] as [string, Identity][])),
      [REACHABLE_WITNESS, witness.identity],
    ]),
    // Every seeded identity carries the chain its own document implies, so no
    // page shows zero entries against a head sequence that is not zero.
    events: new Map([
      [ALICE, aliceEvents()],
      [ACME, acmeEvents()],
      [REACHABLE_WITNESS, witness.events],
      ...(bob === null ? [] : ([[BOB, bob.events]] as [string, LedgerEvent[]][])),
    ]),
    invitations: new Map(),
    contacts,
    graph: { ...seedGraph, roots: seedGraph.roots.map((root) => ({ ...root })) },
    resolved: seedResolved(),
    edges: seedEdges(),
    held,
  };
  persistStore();
}

/**
 * The witness Alice named, as a record this home stored. Its chain publishes
 * one machine, which is what lets its own page say that a machine is listed on
 * its record while the other one it is reachable at is confirmed by nothing.
 */
function storedWitness(): { identity: Identity; events: LedgerEvent[] } {
  const events = witnessEvents();
  const head = events[events.length - 1];
  return {
    events,
    identity: {
      identity_id: REACHABLE_WITNESS,
      declared_kind: "service",
      alias: "",
      created_at_ms: events[0].timestamp_ms,
      head_seq: head.seq,
      head_event: head.event_id,
      event_count: events.length,
      witnesses: [],
      // Its own record names the one machine that answers for it; the other
      // machine the witness list carries is confirmed by nothing this home has.
      endpoints: [WITNESS_MACHINE],
      witness_endpoints: [],
      trust: [],
      principals: [],
      open_invitation_count: 0,
      profile: {
        display_name: WITNESS_NAME,
        hostname: null,
        email: null,
        signing_principal: { identity: REACHABLE_WITNESS, key: events[0].author_key },
        event: events[1].event_id,
        seq: 1,
      },
      verification: { ...UNCLAIMED },
      contact: null,
    },
  };
}

/**
 * Bob as this home stored him: the chain one witness serves, plus the profile
 * the crawl read for him. Every field is a copy of what another route would
 * have written, because the only way to hold a foreign record is to fetch it.
 */
function storedBob(held: HeldLedger[]): { identity: Identity; events: LedgerEvent[] } | null {
  const served = held.find((entry) => entry.entry.ledger_id === BOB);
  if (served === undefined) {
    return null;
  }
  const events = served.events.map((event) => ({ ...event }));
  // The crawl records one attestation of his, the hop that puts Carol two steps
  // from Alice, so his stored record carries the same entry.
  const outgoing = seedEdges().filter((edge) => edge.from === BOB);
  return {
    events,
    identity: {
      identity_id: BOB,
      declared_kind: served.entry.declared_kind,
      alias: "",
      created_at_ms: served.firstSeenMs,
      head_seq: served.entry.head_seq,
      head_event: served.entry.head_event,
      event_count: served.entry.event_count,
      witnesses: [...served.witnesses],
      endpoints: [],
      witness_endpoints: [],
      trust: outgoing.map((edge) => ({
        attestation_event: edge.attestation_event,
        attestation_seq: edge.seq,
        subject: edge.to,
        revoked: false,
        revocation_event: null,
        revocation_seq: null,
      })),
      principals: [],
      open_invitation_count: 0,
      profile: {
        display_name: knownBob.display_name,
        hostname: knownBob.hostname,
        email: knownBob.email,
        signing_principal: { identity: BOB, key: served.events[0].author_key },
        event: served.entry.head_event,
        seq: served.entry.head_seq,
      },
      // He claims a handle and this home has not checked it, which is the
      // unverified state rather than a missing one.
      verification: {
        hostname: knownBob.hostname,
        status: knownBob.verification_status,
        checked_at_ms: null,
        last_verified_at_ms: null,
        stale: true,
        detail: null,
        unreachable: null,
      },
      contact: { ...seedContact },
    },
  };
}

restoreStore();

/** A ledger this home can sign for. Every mutating route goes through it. */
function find(identityId: string): Identity {
  const identity = state.identities.find((entry) => entry.identity_id === identityId);
  if (!identity) {
    failWith(404, {
      ok: false,
      code: 2,
      message: `this home holds no ledger ${identityId}`,
      details: { reason: "unknown_ledger", ledger_id: identityId },
    });
  }
  return identity;
}

/** A ledger this home stored, controlled or fetched. Every read goes through it. */
function findStored(identityId: string): Identity {
  const fetched = state.fetched.get(identityId);
  if (fetched) {
    return fetched;
  }
  return find(identityId);
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
  return { ok: true, identity: findStored(identityId) };
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
  // Both public fields are checked before anything is minted, so a refused
  // email never leaves an identity behind with no profile on it.
  checkDisplayName(body.display_name ?? null);
  checkEmail(body.email ?? null);
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
    endpoints: [],
    witness_endpoints: [],
    trust: [],
    // The inception seeds exactly one principal: the ledger itself on a raw
    // root, the founding identity on an identity root (proposal 002 section 1).
    principals: [
      { identity: founder, active_key: founderKey, role: "controller", is_root: true },
    ],
    open_invitation_count: 0,
    // A new ledger carries no ProfileUpdate yet, so it has no name of its own
    // and nothing to check in DNS.
    profile: null,
    verification: UNCLAIMED,
    contact: null,
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
  // Proposal 005: a public name or email given at creation lands as one
  // ProfileUpdate at seq 1, so a new identity's first two entries are who it is
  // and what it shows the world.
  if (body.display_name !== undefined || body.email !== undefined) {
    replaceProfile(identityId, {
      display_name: body.display_name ?? null,
      hostname: null,
      email: body.email ?? null,
    });
  }
  return { ok: true, identity, inception_event: identityId };
}

/**
 * The two secret keys of one identity. An identity holding no key of its own
 * answers the frozen 409: its controllers sign for it, and their keys belong to
 * their own pages.
 */
export function getIdentityKeys(identityId: string): IdentityKeysResponse {
  const identity = find(identityId);
  if (identity.active_key === undefined || identity.reserve_commit === undefined) {
    failWith(noKeysHeldError.status, {
      ...noKeysHeldError.body,
      message: noKeysHeldError.body.message.replace(ACME, identityId),
      details: { ...noKeysHeldError.body.details, identity_id: identityId },
    });
  }
  return {
    ...identityKeys,
    identity_id: identityId,
    active_key: identity.active_key,
    reserve_commit: identity.reserve_commit,
  };
}

export function getIdentityLedger(
  identityId: string,
  params: { since?: number; limit?: number },
): LedgerPageResponse {
  const identity = findStored(identityId);
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

/**
 * POST /api/identities/:identity_id/witnesses. The array names identity ids and
 * replaces the set whole. An id naming a machine this home knows is refused
 * before any dial, and an id this home can resolve to no identity is refused
 * after trying (proposal 006 section 8).
 */
export function setIdentityWitnesses(identityId: string, witnesses: string[]): AppendResponse {
  const identity = find(identityId);
  const duplicate = witnesses.find((witness, index) => witnesses.indexOf(witness) !== index);
  if (witnesses.length > 16 || duplicate) {
    failWith(400, {
      ok: false,
      code: 10,
      message: "Schema error: witnesses must hold 0 to 16 distinct identity ids",
      details: {
        reason: duplicate ? "duplicate_witness" : "witnesses_out_of_range",
        field: "witnesses",
        value: duplicate ?? String(witnesses.length),
      },
    });
  }
  const machines = knownMachines();
  for (const witness of witnesses) {
    checkIdentityId(witness);
    if (machines.has(witness)) {
      failWith(endpointNotWitnessError.status, {
        ...endpointNotWitnessError.body,
        message: endpointNotWitnessError.body.message.replace(
          String(endpointNotWitnessError.body.details.value),
          witness,
        ),
        details: { ...endpointNotWitnessError.body.details, value: witness },
      });
    }
    const resolvable =
      machinesOf(witness).length > 0 ||
      local(witness) !== undefined ||
      state.fetched.has(witness);
    if (!resolvable) {
      failWith(unresolvableWitnessError.status, {
        ...unresolvableWitnessError.body,
        message: unresolvableWitnessError.body.message.replace(
          String(unresolvableWitnessError.body.details.witness),
          witness,
        ),
        details: { ...unresolvableWitnessError.body.details, witness, endpoints_tried: [] },
      });
    }
  }
  identity.witnesses = [...witnesses];
  const event = append(identity, "witness_set", { witnesses: [...witnesses] });
  return appendResponse(identity, event);
}

/**
 * POST /api/identities/:identity_id/endpoints. One advertisement naming the
 * machines that answer for this identity, replacing the list whole. A
 * replacement that would change nothing is refused, the way the profile route
 * refuses one.
 */
export function setIdentityEndpoints(identityId: string, endpoints: string[]): AppendResponse {
  const identity = find(identityId);
  const duplicate = endpoints.find((machine, index) => endpoints.indexOf(machine) !== index);
  if (endpoints.length > 8 || duplicate) {
    failWith(400, {
      ok: false,
      code: 10,
      message: "Schema error: endpoints must hold 0 to 8 distinct endpoint ids",
      details: {
        reason: duplicate ? "duplicate_endpoint" : "endpoints_out_of_range",
        field: "endpoints",
        value: duplicate ?? String(endpoints.length),
      },
    });
  }
  for (const machine of endpoints) {
    checkEndpointId(machine);
  }
  const current = identity.endpoints;
  if (current.length === endpoints.length && current.every((id, index) => id === endpoints[index])) {
    failWith(noOpEndpointsError.status, {
      ...noOpEndpointsError.body,
      message: noOpEndpointsError.body.message
        .replace(ALICE, identityId)
        .replace(/these \d+ endpoints/, `these ${endpoints.length} endpoints`),
      details: { ...noOpEndpointsError.body.details, identity_id: identityId },
    });
  }
  identity.endpoints = [...endpoints];
  const event = append(identity, "endpoint_advertisement", { endpoints: [...endpoints] });
  return appendResponse(identity, event);
}

function checkEndpointId(endpointId: string): void {
  if (endpointId.length !== 52) {
    failWith(400, {
      ok: false,
      code: 10,
      message: "Schema error: endpoint id must be 52 base32 characters",
      details: { reason: "malformed_endpoint_id", value: endpointId },
    });
  }
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

// The membership routes (ticket 021). Every artifact crosses as base64 of an
// opaque blob; the mock makes that blob a JSON object so an invitation minted
// here can be accepted and admitted here, which is what the harness and the
// component tests drive. A real bundle is a length-prefixed event prefix.

/** The invitation prefix the inviter hands over, carried inside the bundle. */
interface Bundle {
  ledger_id: string;
  declared_kind: DeclaredKind;
  root: RootName;
  controllers: PrincipalEntry[];
  invitation_event: string;
  invitee: string;
  invitee_key: string;
  role: Role;
}

/** The acceptance file the invitee's node signs and hands back. */
interface Acceptance {
  ledger_id: string;
  invitation_event: string;
  invitee: string;
  invitee_key: string;
  role: Role;
}

function encodeArtifact(value: Bundle | Acceptance): string {
  return btoa(JSON.stringify(value));
}

function decodeArtifact<T>(field: string, raw: unknown): T {
  if (typeof raw !== "string" || raw === "") {
    failWith(400, {
      ok: false,
      code: 2,
      message: `${field} is required`,
      details: { reason: "missing_field", field },
    });
  }
  try {
    return JSON.parse(atob(raw)) as T;
  } catch {
    failWith(400, {
      ok: false,
      code: 10,
      message: `Schema error: ${field} is not base64`,
      details: { reason: "malformed_base64", field },
    });
  }
}

function requireField(field: string, value: string | undefined | null): string {
  if (value === undefined || value === null || value === "") {
    failWith(400, {
      ok: false,
      code: 2,
      message: `${field} is required`,
      details: { reason: "missing_field", field },
    });
  }
  return value;
}

/** raw when the ledger keys itself, identity when a founder keys it. */
function rootOf(identity: Identity): RootName {
  return identity.active_key === undefined ? "identity" : "raw";
}

function invitationsOf(ledgerId: string): InvitationEntry[] {
  const held = state.invitations.get(ledgerId);
  if (held) {
    return held;
  }
  const created: InvitationEntry[] = [];
  state.invitations.set(ledgerId, created);
  return created;
}

export function memberships(identityId: string): MembershipView {
  checkIdentityId(identityId);
  const identity = findStored(identityId);
  return {
    ok: true,
    ledger_id: identity.identity_id,
    declared_kind: identity.declared_kind,
    root: rootOf(identity),
    head_seq: identity.head_seq,
    head_event: identity.head_event,
    principals: identity.principals.map((principal) => ({ ...principal })),
    invitations: invitationsOf(identityId).map((entry) => ({ ...entry })),
  };
}

/**
 * A descriptor is opaque bytes the invitee exported. The mock reads the JSON
 * one this store writes and otherwise mints an identity and a key from the
 * bytes, so any uploaded file produces one stable invitee.
 */
function inviteeOf(descriptor: string): { identity: string; active_key: string } {
  try {
    const parsed = JSON.parse(atob(descriptor)) as Partial<{
      identity: string;
      active_key: string;
    }>;
    if (parsed.identity && parsed.active_key) {
      return { identity: parsed.identity, active_key: parsed.active_key };
    }
  } catch {
    // Not the JSON descriptor this store writes, so mint one below.
  }
  return { identity: mintId(descriptor), active_key: mintId(`${descriptor}:key`) };
}

export function invite(identityId: string, body: Partial<InviteRequest>): InvitedResponse {
  const identity = find(identityId);
  const by = requireField("by", body.by);
  const role: Role = body.role === "member" ? "member" : "controller";
  const descriptor = requireField(
    "invitee_descriptor_base64",
    body.invitee_descriptor_base64,
  );
  const invitee = inviteeOf(descriptor);
  const entries = invitationsOf(identityId);
  const open = entries.find(
    (entry) => entry.invitee === invitee.identity && entry.status === "open",
  );
  if (open) {
    failWith(409, {
      ok: false,
      code: 20,
      message: `Policy error: ${invitee.identity} already has an open invitation, ${open.invitation_event}`,
      details: { reason: "duplicate_invitation", at_seq: open.invitation_seq },
    });
  }
  const event = append(identity, "membership_invitation", {
    invitee: invitee.identity,
    invitee_key: invitee.active_key,
    role,
  });
  entries.push({
    invitation_event: event.event_id,
    invitation_seq: event.seq,
    invitee: invitee.identity,
    invitee_key: invitee.active_key,
    role,
    status: "open",
  });
  identity.open_invitation_count += 1;
  const bundle: Bundle = {
    ledger_id: identity.identity_id,
    declared_kind: identity.declared_kind,
    root: rootOf(identity),
    controllers: identity.principals
      .filter((principal) => principal.role === "controller")
      .map((principal) => ({ ...principal })),
    invitation_event: event.event_id,
    invitee: invitee.identity,
    invitee_key: invitee.active_key,
    role,
  };
  return {
    ok: true,
    ledger_id: identity.identity_id,
    by,
    invitee: invitee.identity,
    invitee_key: invitee.active_key,
    role,
    invitation_event: event.event_id,
    invitation_seq: event.seq,
    timestamp_ms: event.timestamp_ms,
    head_seq: identity.head_seq,
    head_event: identity.head_event,
    event,
    invitation_bundle_base64: encodeArtifact(bundle),
    event_count: identity.event_count,
  };
}

export function acceptInvitation(
  identityId: string,
  body: Partial<AcceptRequest>,
): AcceptedResponse {
  checkIdentityId(identityId);
  const bundle = decodeArtifact<Bundle>(
    "invitation_bundle_base64",
    body.invitation_bundle_base64,
  );
  if (bundle.invitee !== identityId) {
    failWith(400, {
      ok: false,
      code: 2,
      message: `this invitation invites ${bundle.invitee}, not ${identityId}`,
      details: {
        reason: "not_the_invitee",
        ledger_id: bundle.ledger_id,
        invitee: bundle.invitee,
      },
    });
  }
  // A controller on a raw-rooted ledger signs as that ledger's own identity,
  // which is the one thing a person must read before accepting.
  const controllerOnRawRoot = bundle.root === "raw" && bundle.role === "controller";
  const acceptance: Acceptance = {
    ledger_id: bundle.ledger_id,
    invitation_event: bundle.invitation_event,
    invitee: bundle.invitee,
    invitee_key: bundle.invitee_key,
    role: bundle.role,
  };
  return {
    ok: true,
    ledger_id: bundle.ledger_id,
    declared_kind: bundle.declared_kind,
    root: bundle.root,
    controllers: bundle.controllers,
    invitation_event: bundle.invitation_event,
    invitee: bundle.invitee,
    invitee_key: bundle.invitee_key,
    role: bundle.role,
    controller_on_raw_root: controllerOnRawRoot,
    warning: controllerOnRawRoot
      ? `accepting a controller role on a raw-rooted ledger means signing as ${bundle.ledger_id}: every event you append to it is that identity's own event`
      : null,
    acceptance_base64: encodeArtifact(acceptance),
  };
}

export function admit(identityId: string, body: Partial<AdmitRequest>): AdmittedResponse {
  const identity = find(identityId);
  const by = requireField("by", body.by);
  const acceptance = decodeArtifact<Acceptance>("acceptance_base64", body.acceptance_base64);
  const entry = invitationsOf(identityId).find(
    (candidate) => candidate.invitation_event === acceptance.invitation_event,
  );
  if (!entry) {
    failWith(409, {
      ok: false,
      code: 20,
      message: `Policy error: Acceptance.invitation_event ${acceptance.invitation_event} names no invitation in this ledger`,
      details: { reason: "unknown_invitation", at_seq: identity.head_seq + 1 },
    });
  }
  if (entry.status !== "open") {
    failWith(409, {
      ok: false,
      code: 50,
      message: `Replay error: this acceptance was already admitted at seq ${entry.invitation_seq} of ${identityId}`,
      details: {
        reason: "acceptance_already_used",
        ledger_id: identityId,
        invitation_event: entry.invitation_event,
        at_seq: entry.invitation_seq,
      },
    });
  }
  const event = append(identity, "membership_acceptance", {
    acceptance: mintId(`${entry.invitation_event}:acceptance`),
    signature: mintId(`${entry.invitation_event}:signature`),
  });
  entry.status = "accepted";
  identity.open_invitation_count = Math.max(0, identity.open_invitation_count - 1);
  identity.principals.push({
    identity: entry.invitee,
    active_key: entry.invitee_key,
    role: entry.role,
    is_root: false,
  });
  return {
    ok: true,
    ledger_id: identity.identity_id,
    by,
    invitee: entry.invitee,
    invitee_key: entry.invitee_key,
    role: entry.role,
    invitation_event: entry.invitation_event,
    acceptance_event: event.event_id,
    acceptance_seq: event.seq,
    timestamp_ms: event.timestamp_ms,
    head_seq: identity.head_seq,
    head_event: identity.head_event,
    event,
  };
}

export function removePrincipal(
  identityId: string,
  body: Partial<RemoveRequest>,
): RemovedResponse {
  const identity = find(identityId);
  const by = requireField("by", body.by);
  const target = requireField("target", body.target);
  const principal = identity.principals.find((entry) => entry.identity === target);
  if (principal?.is_root && rootOf(identity) === "raw") {
    failWith(409, {
      ok: false,
      code: 20,
      message: `Policy error: ${target} is this ledger's raw root and is not removable`,
      details: { reason: "root_not_removable", at_seq: identity.head_seq + 1 },
    });
  }
  const controllers = identity.principals.filter((entry) => entry.role === "controller");
  if (principal?.role === "controller" && controllers.length === 1) {
    failWith(409, {
      ok: false,
      code: 20,
      message: `Policy error: removing ${target} would leave this ledger with no controller`,
      details: { reason: "last_controller", at_seq: identity.head_seq + 1 },
    });
  }
  const open = invitationsOf(identityId).find(
    (entry) => entry.invitee === target && entry.status === "open",
  );
  if (!principal && !open) {
    failWith(409, {
      ok: false,
      code: 20,
      message: `Policy error: ${target} is neither a principal nor an open invitation of ${identityId}`,
      details: { reason: "not_a_principal", ledger_id: identityId, target },
    });
  }
  const event = append(identity, "membership_removal", { target });
  if (principal) {
    identity.principals = identity.principals.filter((entry) => entry.identity !== target);
  }
  if (open) {
    open.status = "cancelled";
    identity.open_invitation_count = Math.max(0, identity.open_invitation_count - 1);
  }
  return {
    ok: true,
    ledger_id: identity.identity_id,
    by,
    target,
    principal_removed: principal !== undefined,
    invitation_cancelled: open?.invitation_event ?? null,
    removal_event: event.event_id,
    removal_seq: event.seq,
    timestamp_ms: event.timestamp_ms,
    head_seq: identity.head_seq,
    head_event: identity.head_event,
    event,
  };
}

export function syncPush(body: Partial<SyncPushRequest>): SyncPushResponse {
  const identity = find(String(body.identity_id));
  // A push goes to the machines the identity's witnesses resolve to, which is
  // what the node does with a witness set (proposal 006 section 5.1).
  const targets = body.to
    ? [{ endpoint_id: body.to, binding: "hinted" as const }]
    : identity.witnesses.flatMap(machinesOf);
  const template = syncPushFixture.results[1];
  // The machine nothing confirms stands in for the unreachable case, so the
  // per-machine table has both outcomes to render.
  const results = targets.map(({ endpoint_id: endpoint, binding }) =>
    endpoint === HINTED_MACHINE || endpoint === UNREACHABLE_MACHINE
      ? {
          ...template,
          endpoint,
          binding,
          message: `Network error: no route to ${endpoint} after 10s`,
        }
      : {
          endpoint,
          binding,
          status: "accepted" as const,
          head_seq: identity.head_seq,
          stored: identity.event_count,
          reject_code: null,
          at_seq: null,
          message: null,
        },
  );
  if (results.every((result) => result.status !== "accepted")) {
    failWith(errors.allWitnessesFailed.status, errors.allWitnessesFailed.body);
  }
  return {
    ok: true,
    ledger_id: identity.identity_id,
    head_seq: identity.head_seq,
    head_event: identity.head_event,
    results,
  };
}

// The witness routes. A witness is an identity, known from a ledger's folded
// witness set or from node.json; there is no directory to ask.

/** The machines the mock knows for one witness identity, in resolution order. */
function machinesOf(identityId: string): WitnessEndpoint[] {
  const frozen = knownWitnesses.find((witness) => witness.identity_id === identityId);
  if (frozen) {
    return frozen.endpoints.map((machine) => ({ ...machine }));
  }
  // A witness named by a ledger and by nothing else: the mock knows of it and
  // knows no machine that answers for it, which is a witness it cannot dial.
  return [];
}

/** Every machine id this home knows, which is what an identity id may not be. */
function knownMachines(): Set<string> {
  const machines = new Set<string>([nodeDocument.endpoint_id, ...nodeDocument.witnesses]);
  for (const witness of knownWitnesses) {
    for (const machine of witness.endpoints) {
      machines.add(machine.endpoint_id);
    }
  }
  return machines;
}

/** GET /api/witnesses, ordered by ascending identity id. */
export function listWitnesses(): WitnessListResponse {
  const table = new Map<string, WitnessSummary>();
  const entry = (identityId: string): WitnessSummary => {
    const held = table.get(identityId);
    if (held) {
      return held;
    }
    const stored = state.fetched.get(identityId) ?? local(identityId);
    const created: WitnessSummary = {
      identity_id: identityId,
      display_name: stored?.profile?.display_name ?? null,
      endpoints: machinesOf(identityId),
      named_by: [],
      is_node_default: false,
      stored: stored !== undefined,
    };
    table.set(identityId, created);
    return created;
  };
  // node.json names one witness identity by default; the seeded home pushes
  // there for any identity that names no witness of its own.
  entry(REACHABLE_WITNESS).is_node_default = true;
  for (const identity of state.identities) {
    for (const witnessId of identity.witnesses) {
      entry(witnessId).named_by.push(identity.identity_id);
    }
  }
  return {
    ok: true,
    witnesses: [...table.values()].sort((left, right) =>
      left.identity_id < right.identity_id ? -1 : 1,
    ),
  };
}

/**
 * GET /api/witnesses/:identity_id/holdings, a live proxy of what that witness
 * keeps. The mock serves one witness's store, and the witness no machine
 * answers for stands in for one this node cannot dial.
 */
export function witnessHoldings(
  identityId: string,
  params: { offset?: number; limit?: number },
): WitnessHoldingsResponse {
  checkIdentityId(identityId);
  if (knownMachines().has(identityId)) {
    failWith(404, {
      ok: false,
      code: 2,
      message: `${identityId} is a machine this home knows, not a witness identity`,
      details: { reason: "endpoint_not_identity", value: identityId },
    });
  }
  const machines = machinesOf(identityId).map((machine) => machine.endpoint_id);
  if (identityId !== REACHABLE_WITNESS) {
    failWith(502, {
      ok: false,
      code: 30,
      message: `Network error: no machine answering for ${identityId} served its ledger list`,
      details: {
        reason: "witness_unreachable",
        identity_id: identityId,
        endpoints_tried: machines,
        error:
          machines.length === 0
            ? "no machine is known for it"
            : `${machines[0]}: no route to ${machines[0]} after 10s`,
      },
    });
  }
  const offset = checkRange("offset", params.offset) ?? 0;
  const limit = checkRange("limit", params.limit) ?? 256;
  const ordered = [...state.held].sort((left, right) =>
    left.entry.ledger_id < right.entry.ledger_id ? -1 : 1,
  );
  return {
    ok: true,
    identity_id: identityId,
    endpoint_id: machines[0],
    offset,
    limit,
    more: ordered.length > offset + limit,
    ledgers: ordered.slice(offset, offset + limit).map(({ entry }) => ({ ...entry })),
  };
}

/** A hostname the mock answers unreachable for, so every verdict is reachable. */
export const UNREACHABLE_HOSTNAME = "unreachable.example";
/** A hostname whose TXT records exist and parse as nothing. */
export const MISMATCHED_HOSTNAME = "mismatched.example";

/**
 * GET /api/resolve?input=. No DNS is queried: the mock answers resolved for a
 * hostname a stored profile claims, and carries one hostname per other verdict
 * so each one renders in the harness and in a test. A Mabel ID and a link are
 * answered from the string, which is what the node does.
 */
export function resolveInput(input: string): ResolveResponse {
  if (/^[a-z2-7]{52}$/i.test(input)) {
    return {
      ok: true,
      input_kind: "identity",
      identity_id: input.toLowerCase(),
      hostname: null,
      endpoints: [],
      status: null,
    };
  }
  if (/:\/\//.test(input) || /^mabel:/i.test(input)) {
    const link = /^mabel:\/\/([a-z2-7]{52})\/?(?:\?endpoints=([a-z2-7]{52}(?:,[a-z2-7]{52}){0,3}))?$/i.exec(
      input,
    );
    if (!link) {
      failWith(400, {
        ok: false,
        code: 2,
        message: `${input} is not a mabel link: it does not begin with mabel://`,
        details: {
          reason: "invalid_mabel_link",
          input,
          detail: "it does not begin with mabel://",
        },
      });
    }
    return {
      ok: true,
      input_kind: "link",
      identity_id: link![1].toLowerCase(),
      hostname: null,
      endpoints: (link![2] ?? "").toLowerCase().split(",").filter(Boolean),
      status: null,
    };
  }
  if (!/^[a-z0-9][a-z0-9.-]*$/.test(input)) {
    failWith(400, {
      ok: false,
      code: 10,
      message: `Schema error: ${input} is not a hostname: it holds a character outside [a-z0-9-]`,
      details: {
        reason: "malformed_hostname",
        value: input,
        detail: "it holds a character outside [a-z0-9-]",
      },
    });
  }
  const hostname = input;
  const claimed = state.identities.find(
    (identity) => identity.profile?.hostname === hostname,
  );
  const answer = (status: ResolveStatus, identityId: string | null): ResolveResponse => ({
    ok: true,
    input_kind: "hostname",
    identity_id: identityId,
    hostname,
    endpoints: [],
    status,
  });
  if (claimed) {
    return answer("resolved", claimed.identity_id);
  }
  if (hostname === UNREACHABLE_HOSTNAME) {
    return answer("unreachable", null);
  }
  if (hostname === MISMATCHED_HOSTNAME) {
    return answer("mismatched_records", null);
  }
  return answer("no_record", null);
}

/**
 * POST /api/identities/:identity_id/fetch. The mock pulls from its own witness
 * store, which is what the witness routes serve, and keeps the result as a
 * stored ledger this home does not control.
 */
export function fetchIdentity(
  identityId: string,
  body: Partial<FetchIdentityRequest>,
): FetchIdentityResponse {
  checkIdentityId(identityId);
  if (body.from && body.from_witness) {
    failWith(400, {
      ok: false,
      code: 2,
      message: "from names an endpoint and from_witness names an identity: give one",
      details: { reason: "conflicting_source", parameter: "from_witness" },
    });
  }
  const source =
    body.from ?? (body.from_witness ? machinesOf(body.from_witness)[0]?.endpoint_id : undefined) ??
    WITNESS_MACHINE;
  const controlled = local(identityId);
  const already = state.fetched.get(identityId);
  if (controlled || already) {
    const held = (controlled ?? already)!;
    return {
      ok: true,
      ledger_id: identityId,
      source,
      event_count: held.event_count,
      stored: 0,
      head_seq: held.head_seq,
      head_event: held.head_event,
      fetched_at_ms: Date.now(),
      controlled_by: controlled ? identityId : null,
    };
  }
  const served = state.held.find((entry) => entry.entry.ledger_id === identityId);
  if (!served) {
    failWith(502, {
      ok: false,
      code: 30,
      message: `Network error: ${source} does not hold ${identityId}`,
      details: { reason: "ledger_not_held", ledger_id: identityId, source },
    });
  }
  const events = served.events.map((event) => ({ ...event }));
  const stored: Identity = {
    identity_id: identityId,
    declared_kind: served.entry.declared_kind,
    alias: "",
    created_at_ms: served.firstSeenMs,
    head_seq: served.entry.head_seq,
    head_event: served.entry.head_event,
    event_count: served.entry.event_count,
    witnesses: [...served.witnesses],
    endpoints: [],
    witness_endpoints: [],
    trust: [],
    principals: [],
    open_invitation_count: 0,
    profile: null,
    verification: { ...UNCLAIMED },
    contact: state.contacts.get(identityId) ?? null,
  };
  state.fetched.set(identityId, stored);
  state.events.set(identityId, events);
  return {
    ok: true,
    ledger_id: identityId,
    source,
    event_count: events.length,
    stored: events.length,
    head_seq: stored.head_seq,
    head_event: stored.head_event,
    fetched_at_ms: Date.now(),
    controlled_by: null,
  };
}

// The profile, verification, contact, lookup and graph routes (proposal 003
// sections 1 to 5). No DNS is resolved and no ledger is fetched here: the mock
// keeps the documents those surfaces read, not the work behind them.

function checkIdentityId(identityId: string): void {
  if (identityId.length !== 52) {
    failWith(400, {
      ok: false,
      code: 10,
      message: "Schema error: identity id must be 52 base32 characters",
      details: { reason: "malformed_identity_id", value: identityId },
    });
  }
}

function local(identityId: string): Identity | undefined {
  return state.identities.find((entry) => entry.identity_id === identityId);
}

/**
 * The name a surface shows for one identity: the profile display name, then the
 * local alias or contact nickname, then nothing, in which case the id is the
 * label (proposal 003 section 4).
 */
function resolve(identityId: string): ResolvedIdentity {
  const nickname = state.contacts.get(identityId)?.nickname ?? null;
  // A stored copy is the better source than the crawl's summary of it, and this
  // home stores both the ledgers it controls and the ones it fetched.
  const held = local(identityId) ?? state.fetched.get(identityId);
  if (held) {
    const displayName = held.profile?.display_name ?? null;
    const alias = nickname ?? (held.alias === "" ? null : held.alias);
    return {
      identity_id: identityId,
      display_name: displayName,
      email: held.profile?.email ?? null,
      alias,
      hostname: held.profile?.hostname ?? null,
      verification_status: held.verification.status,
      provenance: displayName ? "profile" : alias ? "alias" : "none",
    };
  }
  const crawled = state.resolved.get(identityId);
  const alias = nickname ?? crawled?.alias ?? null;
  return {
    identity_id: identityId,
    display_name: crawled?.display_name ?? null,
    email: crawled?.email ?? null,
    alias,
    hostname: crawled?.hostname ?? null,
    verification_status: crawled?.verification_status ?? "unclaimed",
    provenance: crawled?.display_name ? "profile" : alias ? "alias" : "none",
  };
}

/**
 * The edge count from the nearest crawl root, which is every identity this home
 * controls, or null when no crawl reached the identity: "not in my crawl" is an
 * answer and never "no relationship" (contracts/README.md, "Known identities").
 */
function degreesFromRoots(identityId: string): number | null {
  const depth = state.graph?.depth ?? 2;
  let nearest: number | null = null;
  for (const root of state.identities) {
    const trails = shortestTrails(root.identity_id, identityId, depth);
    const length = trails[0]?.length;
    if (length !== undefined && (nearest === null || length < nearest)) {
      nearest = length;
    }
  }
  return nearest;
}

/**
 * GET /api/identities/known. Every identity this home has a record of and does
 * not control, from two local sources: the ledgers it fetched and the crawl
 * generation it stored. Ordered by ascending identity id alone, because an id is
 * the only stable key a row has.
 */
export function listKnownIdentities(params: {
  offset?: number;
  limit?: number;
}): KnownIdentitiesResponse {
  const offset = checkRange("offset", params.offset) ?? 0;
  const limit = checkRange("limit", params.limit) ?? 100;
  const controlled = new Set(state.identities.map((entry) => entry.identity_id));
  const trusted = new Set<string>();
  for (const identity of state.identities) {
    for (const record of identity.trust) {
      if (!record.revoked) {
        trusted.add(record.subject);
      }
    }
  }
  const ids = new Set<string>([...state.fetched.keys(), ...state.resolved.keys()]);
  const identities: KnownIdentity[] = [...ids]
    .filter((identityId) => !controlled.has(identityId))
    .sort((left, right) => (left < right ? -1 : 1))
    .map((identityId) => {
      const stored = state.fetched.get(identityId);
      const named = resolve(identityId);
      return {
        identity_id: identityId,
        display_name: named.display_name,
        alias: named.alias,
        email: named.email,
        hostname: named.hostname,
        verification_status: named.verification_status,
        declared_kind: stored?.declared_kind ?? null,
        stored: stored !== undefined,
        trusted: trusted.has(identityId),
        degrees: degreesFromRoots(identityId),
        head_seq: stored?.head_seq ?? null,
      };
    });
  return {
    ok: true,
    offset,
    limit,
    more: identities.length > offset + limit,
    identities: identities.slice(offset, offset + limit),
  };
}

/** The signer of an append: the mock has one controller per ledger. */
function signingPrincipal(identity: Identity): { identity: string; key: string } {
  const root = identity.principals[0];
  return { identity: root.identity, key: root.active_key };
}

/**
 * The verdict after a profile replacement. An entry is bound to the hostname it
 * verified, so a changed claim starts again at unverified and never inherits
 * the old result (proposal 003 section 2).
 */
function verdictAfterReplacement(current: Verification, hostname: string | null): Verification {
  if (hostname === null) {
    return { ...UNCLAIMED };
  }
  if (hostname === current.hostname) {
    return { ...current };
  }
  return {
    hostname,
    status: "unverified",
    checked_at_ms: null,
    last_verified_at_ms: null,
    stale: true,
    detail: null,
    unreachable: null,
  };
}

/** The profile as it stands now, which is what a replacement diffs against. */
function profileFields(identity: Identity): ProfileFields {
  return {
    display_name: identity.profile?.display_name ?? null,
    hostname: identity.profile?.hostname ?? null,
    email: identity.profile?.email ?? null,
  };
}

/**
 * The canonical encoding forbids a proto3 default on the wire, so a cleared
 * field is simply absent from the payload.
 */
function profilePayload(fields: ProfileFields): Record<string, unknown> {
  return {
    ...(fields.display_name === null ? {} : { display_name: fields.display_name }),
    ...(fields.hostname === null ? {} : { hostname: fields.hostname }),
    ...(fields.email === null ? {} : { email: fields.email }),
  };
}

/**
 * The email shape proposal 005 fixes: at most 254 bytes, exactly one `@` with
 * at least one byte on each side. Deliverability is never checked: the address
 * is a claim on a record, like everything else on one.
 */
function checkEmail(value: string | null): void {
  if (value === null) {
    return;
  }
  const parts = value.split("@");
  const shaped = parts.length === 2 && parts[0].length > 0 && parts[1].length > 0;
  const tooLong = new TextEncoder().encode(value).length > 254;
  if (shaped && !tooLong) {
    return;
  }
  const detail = tooLong
    ? "it is longer than 254 bytes"
    : parts.length < 2
      ? "it holds no at sign"
      : "it needs one at sign with a byte on each side";
  failWith(400, {
    ok: false,
    code: 10,
    message: `Schema error: ProfileUpdate.email is not a valid email: ${detail}`,
    details: { reason: "invalid_email", field: "email", value },
  });
}

/** A display name that parses as an identity id is refused before signing. */
function checkDisplayName(value: string | null): void {
  if (value !== null && /^[a-z2-7]{52}$/.test(value)) {
    failWith(400, {
      ok: false,
      code: 10,
      message: "Schema error: ProfileUpdate.display_name is not a valid name: it parses as an identity id",
      details: { reason: "invalid_display_name", field: "display_name", value },
    });
  }
}

/** POST /api/identities/:identity_id/profile: replacement, never a patch. */
export function replaceProfile(
  identityId: string,
  body: Partial<ProfileFields>,
): ReplaceProfileResponse {
  const identity = find(identityId);
  // Every field is required and may be null: a partial body over a
  // whole-document payload is how a published value disappears by accident.
  for (const field of ["display_name", "hostname", "email"] as const) {
    if (!(field in body)) {
      failWith(400, {
        ok: false,
        code: 2,
        message: `${field} is required`,
        details: { reason: "missing_field", field },
      });
    }
  }
  const displayName = body.display_name ?? null;
  const hostname = body.hostname ?? null;
  const email = body.email ?? null;
  checkDisplayName(displayName);
  checkEmail(email);
  const previous = profileFields(identity);
  if (
    previous.display_name === displayName &&
    previous.hostname === hostname &&
    previous.email === email
  ) {
    failWith(409, {
      ok: false,
      code: 20,
      message: `Policy error: this profile is already the profile of ${identityId}: nothing would change`,
      details: {
        reason: "no_op_profile_update",
        ledger_id: identityId,
        display_name: displayName,
        hostname,
        email,
        profile_event: identity.profile?.event ?? null,
        profile_seq: identity.profile?.seq ?? null,
      },
    });
  }
  const event = append(identity, "profile_update", profilePayload({ display_name: displayName, hostname, email }));
  identity.profile = {
    display_name: displayName,
    hostname,
    email,
    signing_principal: signingPrincipal(identity),
    event: event.event_id,
    seq: event.seq,
  };
  identity.verification = verdictAfterReplacement(identity.verification, hostname);
  return {
    ok: true,
    ledger_id: identityId,
    profile: { ...identity.profile },
    previous,
    head_seq: identity.head_seq,
    head_event: identity.head_event,
    event,
  };
}

/**
 * POST /api/identities/:identity_id/verification. The real node queries
 * _mabel.<hostname> and waits; the mock records a fresh decisive result so the
 * verified and stale-verified states are both reachable in the harness.
 */
export function forceVerification(identityId: string): VerificationResponse {
  const identity = find(identityId);
  const hostname = identity.profile?.hostname ?? null;
  if (hostname === null) {
    failWith(409, {
      ok: false,
      code: 20,
      message: `Policy error: ${identityId} claims no hostname, so there is nothing to check`,
      details: { reason: "no_hostname_claimed", identity_id: identityId },
    });
  }
  const checkedAt = Date.now();
  identity.verification = {
    hostname,
    status: "verified",
    checked_at_ms: checkedAt,
    last_verified_at_ms: checkedAt,
    stale: false,
    detail: `_mabel.${hostname}. answers mabel=${identityId}`,
    unreachable: null,
  };
  return { ok: true, identity_id: identityId, verification: { ...identity.verification } };
}

const CONTACT_CAPS = { nickname: 64, note: 512 } as const;

export function getContact(identityId: string): ContactResponse {
  checkIdentityId(identityId);
  const contact = state.contacts.get(identityId);
  return { ok: true, identity_id: identityId, contact: contact ? { ...contact } : null };
}

/** PUT replaces the contact document whole; both fields null clears it. */
export function setContact(
  identityId: string,
  body: Partial<SetContactRequest>,
): ContactResponse {
  checkIdentityId(identityId);
  const nickname = body.nickname ?? null;
  const note = body.note ?? null;
  for (const [field, value] of [
    ["nickname", nickname],
    ["note", note],
  ] as const) {
    if (value === null) {
      continue;
    }
    const length = new TextEncoder().encode(value).length;
    if (length > CONTACT_CAPS[field]) {
      failWith(400, {
        ok: false,
        code: 10,
        message: `Schema error: ${field} is at most ${CONTACT_CAPS[field]} bytes of UTF-8, and this is ${length}`,
        details: {
          reason: "contact_field_too_long",
          field,
          len: length,
          cap: CONTACT_CAPS[field],
        },
      });
    }
  }
  const held = local(identityId) ?? state.fetched.get(identityId);
  if (nickname === null && note === null) {
    state.contacts.delete(identityId);
    if (held) {
      held.contact = null;
    }
    return { ok: true, identity_id: identityId, contact: null };
  }
  const contact: Contact = { nickname, note, updated_at_ms: Date.now() };
  state.contacts.set(identityId, contact);
  if (held) {
    held.contact = { ...contact };
  }
  return { ok: true, identity_id: identityId, contact: { ...contact } };
}

function equivocationFor(identityId: string) {
  if (!state.graph?.equivocations.includes(identityId)) {
    return null;
  }
  return seedLookup.equivocation ? { ...seedLookup.equivocation } : null;
}

function hopOf(edge: Edge): LookupHop {
  return {
    from: resolve(edge.from),
    to: resolve(edge.to),
    attestation_event: edge.attestation_event,
    fetched_at_ms: state.graph?.last_sync_ms ?? 0,
    stale: state.graph?.stale ?? false,
    equivocation: equivocationFor(edge.to),
  };
}

/** Up to three shortest trails from one root, breadth first and depth capped. */
function shortestTrails(from: string, to: string, maxDepth: number): Edge[][] {
  if (from === to) {
    return [];
  }
  const found: Edge[][] = [];
  const seen = new Set([from]);
  let level: { id: string; trail: Edge[] }[] = [{ id: from, trail: [] }];
  for (let depth = 0; depth < maxDepth && found.length === 0 && level.length > 0; depth += 1) {
    const next: { id: string; trail: Edge[] }[] = [];
    for (const node of level) {
      for (const edge of state.edges.filter((entry) => entry.from === node.id)) {
        if (seen.has(edge.to)) {
          continue;
        }
        const trail = [...node.trail, edge];
        if (edge.to === to) {
          found.push(trail);
          continue;
        }
        next.push({ id: edge.to, trail });
      }
    }
    for (const node of next) {
      seen.add(node.id);
    }
    level = next;
  }
  return found.slice(0, 3);
}

/**
 * GET /api/lookup/:identity_id?from=. An identity the crawl never reached is a
 * 200 with degrees null and no paths: "not in my crawl" is an answer.
 */
export function lookup(identityId: string, params: { from?: string }): LookupResponse {
  checkIdentityId(identityId);
  const ordered = [...state.identities].sort((left, right) =>
    left.identity_id < right.identity_id ? -1 : 1,
  );
  // The node defaults from to the lowest local identity id; the wallet sends
  // the identity its selector holds.
  const from = params.from ?? ordered[0]?.identity_id;
  if (from === undefined || !local(from)) {
    failWith(400, {
      ok: false,
      code: 2,
      message: `no identity here is named ${from}`,
      details: { reason: "unknown_from_identity", parameter: "from", value: String(from) },
    });
  }
  const trails = shortestTrails(from, identityId, state.graph?.depth ?? 2);
  return {
    ok: true,
    identity: resolve(identityId),
    from: resolve(from),
    degrees: trails.length > 0 ? trails[0].length : null,
    paths: trails.map((trail) => ({ hops: trail.map(hopOf) })),
    trust: state.edges
      .filter((edge) => edge.from === identityId)
      .map((edge) => ({
        subject: resolve(edge.to),
        attestation_event: edge.attestation_event,
        seq: edge.seq,
      })),
    reverse: {
      best_effort: true,
      entries: state.edges
        .filter((edge) => edge.to === identityId)
        .map((edge) => ({
          identity: resolve(edge.from),
          attestation_event: edge.attestation_event,
          seq: edge.seq,
        })),
    },
    equivocation: equivocationFor(identityId),
    fetched_at_ms: state.graph?.last_sync_ms ?? null,
    stale: state.graph?.stale ?? false,
    sync_id: state.graph?.sync_id ?? null,
    last_sync_ms: state.graph?.last_sync_ms ?? null,
    graph_stale: state.graph?.stale ?? false,
    graph_truncated: state.graph?.truncated ?? false,
    truncated_by: state.graph?.truncated_by ?? null,
  };
}

function cloneGraph(graph: Graph): Graph {
  return { ...graph, roots: graph.roots.map((root) => ({ ...root })), equivocations: [...graph.equivocations] };
}

export function getGraph(): GraphResponse {
  return { ok: true, graph: state.graph ? cloneGraph(state.graph) : null };
}

/** POST /api/graph/sync mints one generation; synchronizing is always manual. */
export function syncGraph(): GraphSyncResponse {
  if (state.identities.length === 0) {
    failWith(400, {
      ok: false,
      code: 2,
      message: "this home holds no identity to crawl from",
      details: { reason: "no_local_identity" },
    });
  }
  const now = Date.now();
  const nodes = new Set<string>(state.identities.map((entry) => entry.identity_id));
  for (const edge of state.edges) {
    nodes.add(edge.from);
    nodes.add(edge.to);
  }
  state.graph = {
    sync_id: `${now}-${mintId("sync").slice(0, 5)}`,
    last_sync_ms: now,
    depth: seedGraph.depth,
    // Every local identity is a crawl root at depth 0 (proposal 003 section 3).
    roots: [...state.identities]
      .sort((left, right) => (left.identity_id < right.identity_id ? -1 : 1))
      .map((entry) => resolve(entry.identity_id)),
    node_count: nodes.size,
    edge_count: state.edges.length,
    fetch_count: nodes.size,
    truncated: seedGraph.truncated,
    truncated_by: seedGraph.truncated_by,
    equivocations: [...seedGraph.equivocations],
    stale: false,
  };
  return { ok: true, graph: cloneGraph(state.graph) };
}

/**
 * GET /api/node: one document, whatever this home does. The counts follow the
 * seeded store rather than the fixture, so the node page and the lists agree.
 */
export function nodeInfo(): NodeInfo {
  return {
    ...nodeDocument,
    identity_count: state.identities.length,
    ledger_count: state.identities.length + state.fetched.size,
  };
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
