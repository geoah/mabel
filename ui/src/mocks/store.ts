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
  ForkListResponse,
  ForkRecord,
  Graph,
  GraphResponse,
  GraphSyncResponse,
  Identity,
  IdentityListResponse,
  IdentityResponse,
  InvitationEntry,
  InviteRequest,
  InvitedResponse,
  LedgerEntryResponse,
  LedgerEvent,
  LedgerListResponse,
  LedgerPageResponse,
  LookupHop,
  LookupResponse,
  MembershipView,
  PrincipalEntry,
  ProfileFields,
  RemoveRequest,
  RemovedResponse,
  ReplaceProfileResponse,
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
  seedContact,
  seedEdges,
  seedEvents,
  seedGraph,
  seedIdentities,
  seedLookup,
  seedResolved,
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

/** One crawled attestation: who attests, to whom, with which event. */
interface Edge {
  from: string;
  to: string;
  attestation_event: string;
  seq: number;
}

interface State {
  identities: Identity[];
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
  /** The witness side, independent of the wallet: a witness holds copies. */
  held: HeldLedger[];
  forks: ForkRecord[];
}

let state: State = emptyState();

function emptyState(): State {
  return {
    identities: [],
    events: new Map(),
    invitations: new Map(),
    contacts: new Map(),
    graph: null,
    resolved: new Map(),
    edges: [],
    held: [],
    forks: [],
  };
}

export function resetStore(): void {
  minted = 0;
  nodeRole = null;
  const contacts = new Map<string, Contact>();
  // Bob is a foreign identity with a local note: the contact store covers ids
  // this home does not control.
  contacts.set(BOB, { ...seedContact });
  const identities = seedIdentities.map((identity) => ({
    ...identity,
    witnesses: [...identity.witnesses],
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
  state = {
    identities,
    events: new Map([[ALICE, seedEvents()]]),
    invitations: new Map(),
    contacts,
    graph: { ...seedGraph, roots: seedGraph.roots.map((root) => ({ ...root })) },
    resolved: seedResolved(),
    edges: seedEdges(),
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
      message: `this home holds no ledger ${identityId}`,
      details: { reason: "unknown_ledger", ledger_id: identityId },
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

// The membership routes (ticket 021). Every artifact crosses as base64 of an
// opaque blob; the mock makes that blob a JSON object so an invitation minted
// here can be accepted and admitted here, which is what the demo and the
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
  const identity = find(identityId);
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
  const held = local(identityId);
  if (held) {
    const displayName = held.profile?.display_name ?? null;
    const alias = nickname ?? held.alias;
    return {
      identity_id: identityId,
      display_name: displayName,
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
    alias,
    hostname: crawled?.hostname ?? null,
    verification_status: crawled?.verification_status ?? "unclaimed",
    provenance: crawled?.display_name ? "profile" : alias ? "alias" : "none",
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
  for (const field of ["display_name", "hostname"] as const) {
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
  checkDisplayName(displayName);
  const previous: ProfileFields = {
    display_name: identity.profile?.display_name ?? null,
    hostname: identity.profile?.hostname ?? null,
  };
  if (previous.display_name === displayName && previous.hostname === hostname) {
    failWith(409, {
      ok: false,
      code: 20,
      message: `Policy error: this profile is already the profile of ${identityId}: nothing would change`,
      details: {
        reason: "no_op_profile_update",
        ledger_id: identityId,
        display_name: displayName,
        hostname,
        profile_event: identity.profile?.event ?? null,
        profile_seq: identity.profile?.seq ?? null,
      },
    });
  }
  // The canonical encoding forbids a proto3 default on the wire, so a cleared
  // name is simply absent from the payload.
  const event = append(identity, "profile_update", {
    ...(displayName === null ? {} : { display_name: displayName }),
    ...(hostname === null ? {} : { hostname }),
  });
  identity.profile = {
    display_name: displayName,
    hostname,
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
 * verified and stale-verified states are both reachable in dev and demo mode.
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
  const held = local(identityId);
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
