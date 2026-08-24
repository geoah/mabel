// The frozen fixtures, imported straight from contracts/http and contracts/cli
// so the contract stays the single source of truth. Nothing here is edited: the
// casts only narrow the structural types TypeScript infers from JSON onto the
// hand-written contract types in src/api/types.ts.

import nodeFixture from "@contracts/http/node-get-node.json";
import identitiesFixture from "@contracts/http/wallet-get-identities.json";
import knownIdentitiesFixture from "@contracts/http/wallet-get-known-identities.json";
import profileFixture from "@contracts/http/wallet-post-identity-profile.json";
import verificationFixture from "@contracts/http/wallet-post-identity-verification.json";
import contactFixture from "@contracts/http/wallet-get-identity-contact.json";
import contactPutFixture from "@contracts/http/wallet-put-identity-contact.json";
import identityKeysFixture from "@contracts/http/wallet-get-identity-keys.json";
import lookupFixture from "@contracts/http/wallet-get-lookup.json";
import graphFixture from "@contracts/http/wallet-get-graph.json";
import createIdentityFixture from "@contracts/http/wallet-post-identities.json";
import witnessesFixture from "@contracts/http/wallet-post-identity-witnesses.json";
import trustFixture from "@contracts/http/wallet-post-trust.json";
import revokeFixture from "@contracts/http/wallet-post-trust-revoke.json";
import syncPushFixture from "@contracts/http/wallet-post-sync-push.json";
import knownWitnessesFixture from "@contracts/http/wallet-get-witnesses.json";
import witnessHoldingsFixture from "@contracts/http/wallet-get-witness-holdings.json";
import endpointsFixture from "@contracts/http/wallet-post-identity-endpoints.json";
import ledgerFixture from "@contracts/http/wallet-get-identity-ledger.json";
import resolveFixture from "@contracts/http/wallet-get-resolve.json";
import fetchFixture from "@contracts/http/wallet-post-identity-fetch.json";
import cliErrors from "@contracts/cli/errors.json";

import type {
  AppendResponse,
  Contact,
  CreateIdentityResponse,
  DeclaredKind,
  ErrorEnvelope,
  Graph,
  Identity,
  IdentityKeysResponse,
  KnownIdentity,
  LedgerEvent,
  LedgerPageResponse,
  LookupResponse,
  NodeInfo,
  ReplaceProfileResponse,
  ResolveResponse,
  ResolvedIdentity,
  RevokeTrustResponse,
  SyncPushResponse,
  Verification,
  WitnessLedgerSummary,
  WitnessSummary,
} from "@/api/types";

/** The ids the fixtures share with test-vectors/. */
export const ALICE = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";
export const BOB = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";
export const ACME = "2okqwhextnpkpmydrgrkk563vbehcklffwfzidxlh5dslawjmn6a";

/** GET /api/node: one document, on every node (proposal 006 section 8). */
export const nodeDocument = nodeFixture.response as NodeInfo;

export const seedIdentities = identitiesFixture.response.identities as Identity[];
export const createdIdentity = createIdentityFixture.response as CreateIdentityResponse;
export const witnessConfigAppend = witnessesFixture.response as AppendResponse;
export const trustAppend = trustFixture.response as AppendResponse;
export const trustRevoke = revokeFixture.response as RevokeTrustResponse;
export const syncPush = syncPushFixture.response as SyncPushResponse;

/** GET /api/witnesses, the witness identities the frozen answer names. */
export const knownWitnesses = knownWitnessesFixture.response.witnesses as WitnessSummary[];
/**
 * The witness the drill-in answers for: the identity the frozen holdings page
 * was asked about. This home stores its record, so its own page shows the
 * machines its record lists as well as the one nothing confirms.
 */
export const REACHABLE_WITNESS = witnessHoldingsFixture.response.identity_id;
/**
 * The other witness the frozen list carries. The mock hands it to acme and no
 * machine answers for it, so both the "not a node default" marker and the
 * unreachable drill-in have a case.
 */
export const UNREACHABLE_WITNESS = knownWitnesses.find(
  (witness) => witness.identity_id !== REACHABLE_WITNESS,
)!.identity_id;
/** The machines the frozen list gives the reachable witness, in its own order. */
export const WITNESS_MACHINES = knownWitnesses.find(
  (witness) => witness.identity_id === REACHABLE_WITNESS,
)!.endpoints;
/** The one machine the reachable witness's own record lists. */
export const WITNESS_MACHINE = WITNESS_MACHINES.find(
  (machine) => machine.binding === "verified",
)!.endpoint_id;
/** The machine nothing this home holds confirms, kept for the second sentence. */
export const HINTED_MACHINE = WITNESS_MACHINES.find(
  (machine) => machine.binding === "hinted",
)!.endpoint_id;
/** The single machine the unreachable witness is known at. */
export const UNREACHABLE_MACHINE = knownWitnesses.find(
  (witness) => witness.identity_id === UNREACHABLE_WITNESS,
)!.endpoints[0].endpoint_id;

/**
 * POST /api/identities/:identity_id/witnesses refuses an id at a value surface
 * that names no identity this home can reach, and one that names a machine.
 * Both messages are the node's own, reproduced here from the frozen answer.
 */
export const unresolvableWitnessError = witnessesFixture.errors.find(
  (error) => error.body.details.reason === "unresolvable_witness",
)! as { status: number; body: ErrorEnvelope };
export const endpointNotWitnessError = witnessesFixture.errors.find(
  (error) => error.body.details.reason === "endpoint_not_identity",
)! as { status: number; body: ErrorEnvelope };

/** POST /api/identities/:identity_id/endpoints, refusing a no-op replacement. */
export const noOpEndpointsError = endpointsFixture.errors.find(
  (error) => error.body.details.reason === "no_op_endpoint_advertisement",
)! as { status: number; body: ErrorEnvelope };

/** GET /api/witnesses/:identity_id/holdings, unreachable: 502 witness_unreachable. */
export const witnessUnreachableError = witnessHoldingsFixture.errors.find(
  (error) => error.body.details.reason === "witness_unreachable",
)! as { status: number; body: ErrorEnvelope };
/** The same route, given a machine id where an identity id belongs: 404. */
export const endpointNotIdentityError = witnessHoldingsFixture.errors.find(
  (error) => error.body.details.reason === "endpoint_not_identity",
)! as { status: number; body: ErrorEnvelope };

/** GET /api/resolve?input=, the hostname alice's profile claims. */
export const resolvedHostname = resolveFixture.response as ResolveResponse;
/** POST /api/identities/:identity_id/fetch, refused because no source holds it. */
export const fetchNotHeldError = fetchFixture.errors.find(
  (error) => error.body.details.reason === "ledger_not_held",
)! as { status: number; body: ErrorEnvelope };

/** POST /api/identities/:identity_id/profile, the whole-document replacement. */
export const profileReplaced = profileFixture.response as ReplaceProfileResponse;
/** POST /api/identities/:identity_id/verification, a forced check that verified. */
export const forcedVerification = verificationFixture.response.verification as Verification;
/** The local contact note the fixtures hold for Bob. */
export const seedContact = contactFixture.response.contact as Contact;
export const contactRoundTrip = contactPutFixture.request as {
  nickname: string;
  note: string;
};
/**
 * GET /api/identities/:identity_id/keys, the pair the frozen answer hands back.
 * The mock reuses these two secrets for every raw-rooted identity it holds: it
 * runs no crypto, so a secret it minted itself would say nothing more.
 */
export const identityKeys = identityKeysFixture.response as IdentityKeysResponse;
/** code 20 at 409, the keys of an identity that holds none of its own. */
export const noKeysHeldError = refusal(identityKeysFixture.errors, "no_keys_held");

/** GET /api/identities/known, the two rows the frozen answer carries. */
export const knownIdentityRows = knownIdentitiesFixture.response
  .identities as KnownIdentity[];
/**
 * The row the contract pins for Bob: stored, trusted, one degree away. The mock
 * seeds his stored copy from these values, so the harness's known list reads
 * the way the frozen answer does.
 */
export const knownBob = knownIdentityRows.find((row) => row.identity_id === BOB)!;

/** GET /api/lookup/:identity_id, the two-hop answer from Alice to Carol. */
export const seedLookup = lookupFixture.response as LookupResponse;
/** GET /api/graph, the crawl generation this home last recorded. */
export const seedGraph = graphFixture.response.graph as Graph;

/** The foreign identity the lookup fixture answers for, two hops from Alice. */
export const CAROL = seedLookup.identity.identity_id;

/**
 * Every ResolvedIdentity the lookup and graph fixtures carry, keyed by id, so
 * the mock names a foreign identity exactly the way the contract does.
 */
export function seedResolved(): Map<string, ResolvedIdentity> {
  const table = new Map<string, ResolvedIdentity>();
  const add = (resolved: ResolvedIdentity) => table.set(resolved.identity_id, { ...resolved });
  add(seedLookup.identity);
  add(seedLookup.from);
  for (const path of seedLookup.paths) {
    for (const hop of path.hops) {
      add(hop.from);
      add(hop.to);
    }
  }
  for (const entry of seedLookup.trust) {
    add(entry.subject);
  }
  for (const entry of seedLookup.reverse.entries) {
    add(entry.identity);
  }
  for (const root of seedGraph.roots) {
    add(root);
  }
  return table;
}

/**
 * The crawled edges the lookup fixture implies: Alice attests to Bob, Bob to
 * Carol, and Carol back to Bob. Attestation event ids come from the fixture so
 * a rendered path quotes the contract's own values.
 */
export function seedEdges(): { from: string; to: string; attestation_event: string; seq: number }[] {
  const [aliceToBob, bobToCarol] = seedLookup.paths[0].hops;
  return [
    {
      from: aliceToBob.from.identity_id,
      to: aliceToBob.to.identity_id,
      attestation_event: aliceToBob.attestation_event,
      seq: 8,
    },
    {
      from: bobToCarol.from.identity_id,
      to: bobToCarol.to.identity_id,
      attestation_event: bobToCarol.attestation_event,
      seq: seedLookup.reverse.entries[0].seq,
    },
    {
      from: seedLookup.identity.identity_id,
      to: seedLookup.trust[0].subject.identity_id,
      attestation_event: seedLookup.trust[0].attestation_event,
      seq: seedLookup.trust[0].seq,
    },
  ];
}

/** One frozen refusal of one route, found by the reason it carries. */
function refusal(
  cases: { status: number; body: { details: { reason: string } } }[],
  reason: string,
): { status: number; body: ErrorEnvelope } {
  return cases.find((entry) => entry.body.details.reason === reason)! as {
    status: number;
    body: ErrorEnvelope;
  };
}

/** Named error bodies the handlers and the tests reuse, one per exit-code class. */
export const errors = {
  /** code 2, no layer prefix. */
  missingField: refusal(createIdentityFixture.errors, "missing_field"),
  /** code 10, Schema error. */
  unknownEnumValue: refusal(createIdentityFixture.errors, "unknown_enum_value"),
  /** code 70, no layer prefix. */
  unsupportedDeclaredKind: refusal(createIdentityFixture.errors, "unsupported_declared_kind"),
  /** code 20, Policy error. */
  duplicateAttestation: refusal(trustFixture.errors, "duplicate_unrevoked_attestation"),
  /** code 2, an identity this node home holds no ledger for. */
  unknownLedger: refusal(trustFixture.errors, "unknown_ledger"),
  /** code 30, Network error. */
  allWitnessesFailed: refusal(syncPushFixture.errors, "all_witnesses_failed"),
  /** code 50, State error. */
  staleHead: refusal(witnessesFixture.errors, "stale_head"),
  /** code 60, insecure key permissions, no layer prefix. */
  insecureKeyPermissions: refusal(identitiesFixture.errors, "insecure_key_permissions"),
  /** code 2, a profile body that names only one of the two keys. */
  profileMissingField: refusal(profileFixture.errors, "missing_field"),
  /** code 20 at 409, a replacement that would change nothing. */
  noOpProfileUpdate: refusal(profileFixture.errors, "no_op_profile_update"),
  /** code 10, a display name that parses as an identity id. */
  invalidDisplayName: refusal(profileFixture.errors, "invalid_display_name"),
  /** code 10, an email with no at sign (proposal 005). */
  invalidEmail: refusal(profileFixture.errors, "invalid_email"),
  /** code 20 at 409, a forced check on an identity claiming no hostname. */
  noHostnameClaimed: refusal(verificationFixture.errors, "no_hostname_claimed"),
  /** code 10, a contact nickname past its 64-byte cap. */
  contactFieldTooLong: refusal(contactPutFixture.errors, "contact_field_too_long"),
  /** code 2, a lookup whose from names no identity in this home. */
  unknownFromIdentity: refusal(lookupFixture.errors, "unknown_from_identity"),
} as const;

/**
 * contracts/cli/errors.json, one case per exit code and layer prefix. The same
 * envelope crosses HTTP, so these double as the HTTP error bodies.
 */
export const cliErrorCases = cliErrors.cases as {
  case: string;
  exit_code: number;
  document: ErrorEnvelope;
}[];

// The chains behind the frozen documents. GET /api/identities/:id/ledger is the
// one route serving events now, and its fixture is a page from seq 2, so the
// entries before and after it are minted here to agree with the identity
// document: nine entries for Alice, head at 8, the profile at 7.

/** The page the ledger route freezes: Alice's entries 2 and 3. */
export const seedLedgerPage = ledgerFixture.response as LedgerPageResponse;

/** The base32 id of a minted event, distinct per ledger and per position. */
function syntheticEventId(tag: string, marker: string): string {
  return `${tag}${marker}`.repeat(18).slice(0, 52);
}

const POSITION_MARKERS = "abcdefghijklmnop";
/** The nonce every minted inception carries; the mock runs no crypto. */
const INCEPTION_NONCE = syntheticEventId("no", "nc");

const alice = seedIdentities.find((identity) => identity.identity_id === ALICE)!;
const acme = seedIdentities.find((identity) => identity.identity_id === ACME)!;
const AUTHOR_KEY = alice.active_key!;

/** The inception of a raw-rooted ledger: its event id is the ledger id. */
function rawInception(
  ledgerId: string,
  declaredKind: DeclaredKind,
  timestampMs: number,
  activeKey: string,
  reserveCommit: string,
): LedgerEvent {
  return {
    event_id: ledgerId,
    seq: 0,
    ledger_id: null,
    prev: null,
    timestamp_ms: timestampMs,
    author_key: activeKey,
    payload_kind: "inception",
    payload: {
      declared_kind: declaredKind,
      nonce: INCEPTION_NONCE,
      root: { raw_root: { active_key: activeKey, reserve_commit: reserveCommit } },
    },
  };
}

/** Links each entry to the one before it, once every event id is known. */
function chained(events: LedgerEvent[]): LedgerEvent[] {
  for (const event of events.slice(1)) {
    event.prev = events[event.seq - 1].event_id;
  }
  return events;
}

/**
 * Alice's chain as her own wallet holds it: nine entries, the frozen page's two
 * among them. Her document reports nine entries and a head at 8, so a shorter
 * chain would render fewer entries than the head claims, which is the thing
 * decision 017 refuses to show.
 *
 * The minted entries are the ones the folded state demands: the witness set she
 * names at 1, her profile at 7, where the document says its event is, and the
 * attestation naming Bob at 8, where the document's head event is.
 */
export function aliceEvents(): LedgerEvent[] {
  const trusted = alice.trust.find((record) => !record.revoked)!;
  const profile = alice.profile!;
  const after = (
    seq: number,
    eventId: string,
    payloadKind: string,
    payload: Record<string, unknown>,
  ): LedgerEvent => ({
    event_id: eventId,
    seq,
    ledger_id: ALICE,
    prev: "",
    timestamp_ms: alice.created_at_ms + seq * 60000,
    author_key: AUTHOR_KEY,
    payload_kind: payloadKind,
    payload,
  });

  return chained([
    rawInception(ALICE, alice.declared_kind, alice.created_at_ms, AUTHOR_KEY, alice.reserve_commit!),
    // The three the ledger route freezes: the witness set she names, the
    // attestation naming Bob and the revocation that took it back.
    ...seedLedgerPage.events.map((event) => ({ ...event, payload: { ...event.payload } })),
    // A name typed, corrected, then a website added: four replacements, because
    // a profile update always carries both fields.
    after(4, syntheticEventId("al", "e"), "profile_update", {
      display_name: "Alice",
      hostname: null,
    }),
    after(5, syntheticEventId("al", "f"), "profile_update", {
      display_name: "Alice A.",
      hostname: null,
    }),
    after(6, syntheticEventId("al", "g"), "profile_update", {
      display_name: profile.display_name,
      hostname: null,
    }),
    after(7, profile.event, "profile_update", {
      display_name: profile.display_name,
      hostname: profile.hostname,
    }),
    after(8, trusted.attestation_event, "trust_attestation", { subject: trusted.subject }),
  ]);
}

/**
 * Acme's chain, built to agree with the identity document the frozen list
 * carries for it: five entries, head at 4, the witness it names, and the
 * display name its profile reports, published by the last entry so
 * `profile.seq` and `profile.event` land on the head.
 *
 * Acme is identity-rooted, so entry 0 is an inception naming its founder rather
 * than a key of its own, and every entry is signed by that founder's key. The
 * mock ran no crypto to make these, and they exist so a page about Acme shows a
 * record instead of no entries against a head that is not zero.
 */
export function acmeEvents(): LedgerEvent[] {
  const founder = acme.principals[0];
  const shape = (
    seq: number,
    eventId: string,
    payloadKind: string,
    payload: Record<string, unknown>,
  ): LedgerEvent => ({
    event_id: eventId,
    seq,
    ledger_id: seq === 0 ? null : ACME,
    prev: seq === 0 ? null : "",
    timestamp_ms: acme.created_at_ms + seq * 60000,
    author_key: founder.active_key,
    payload_kind: payloadKind,
    payload,
  });

  return chained([
    // Entry 0 is the inception, and its event id is the identity id.
    shape(0, ACME, "inception", {
      declared_kind: acme.declared_kind,
      nonce: INCEPTION_NONCE,
      root: {
        identity_root: {
          founder: founder.identity,
          founder_key: founder.active_key,
          founder_inception: founder.identity,
        },
      },
    }),
    shape(1, syntheticEventId("ac", "b"), "witness_set", {
      witnesses: [UNREACHABLE_WITNESS],
    }),
    shape(2, syntheticEventId("ac", "c"), "profile_update", {
      display_name: "Acme",
      hostname: null,
    }),
    shape(3, syntheticEventId("ac", "d"), "profile_update", {
      display_name: "Acme Corp",
      hostname: null,
    }),
    // The head, so profile.event and profile.seq of the document agree with it.
    shape(4, acme.head_event, "profile_update", {
      display_name: acme.profile!.display_name,
      hostname: null,
    }),
  ]);
}

// The reachable witness, as a record this home stored: an identity like any
// other, whose chain publishes the one machine that answers for it. It is what
// makes a witness's own page show a machine its record lists beside a machine
// nothing confirms.

/** When the witness identity was created, an hour before Alice's first entry. */
const WITNESS_CREATED_MS = alice.created_at_ms - 3600000;
/** The key the witness identity signs with; the mock mints no keys. */
const WITNESS_KEY = syntheticEventId("wi", "k");
/** The reserve commitment of the same identity, likewise minted. */
const WITNESS_COMMIT = syntheticEventId("wi", "r");
/** What the witness identity's profile publishes, from the frozen witness list. */
export const WITNESS_NAME = knownWitnesses.find(
  (witness) => witness.identity_id === REACHABLE_WITNESS,
)!.display_name;

/** The witness identity's own chain: it exists, it is named, it publishes a machine. */
export function witnessEvents(): LedgerEvent[] {
  const shape = (
    seq: number,
    eventId: string,
    payloadKind: string,
    payload: Record<string, unknown>,
  ): LedgerEvent => ({
    event_id: eventId,
    seq,
    ledger_id: REACHABLE_WITNESS,
    prev: "",
    timestamp_ms: WITNESS_CREATED_MS + seq * 60000,
    author_key: WITNESS_KEY,
    payload_kind: payloadKind,
    payload,
  });

  return chained([
    rawInception(REACHABLE_WITNESS, "service", WITNESS_CREATED_MS, WITNESS_KEY, WITNESS_COMMIT),
    shape(1, syntheticEventId("wi", "b"), "profile_update", {
      display_name: WITNESS_NAME,
      hostname: null,
    }),
    shape(2, syntheticEventId("wi", "c"), "endpoint_advertisement", {
      endpoints: [WITNESS_MACHINE],
    }),
  ]);
}

// What one witness holds, which is what GET /api/witnesses/:identity_id/holdings
// proxies. The two frozen entries page in one request, so the mock adds four
// more ledgers to give the list a second page.

export const witnessLedgerEntries = witnessHoldingsFixture.response
  .ledgers as WitnessLedgerSummary[];

/** One ledger a witness holds: the summary it serves, its witnesses and its chain. */
export interface HeldLedger {
  entry: WitnessLedgerSummary;
  /** When the witness first saw it, which a fetched copy takes as its created date. */
  firstSeenMs: number;
  /** The witness identities the ledger's own record names. */
  witnesses: string[];
  events: LedgerEvent[];
}

/**
 * A chain shaped like Alice's for a ledger the fixtures do not carry: seq 0 is
 * the inception whose event id is the ledger id, and the last event id is the
 * summary's head_event, so the summary and the chain agree.
 */
function syntheticEvents(entry: WitnessLedgerSummary, firstSeenMs: number): LedgerEvent[] {
  const tag = entry.ledger_id.slice(0, 2);
  const events: LedgerEvent[] = [
    rawInception(entry.ledger_id, entry.declared_kind, firstSeenMs, AUTHOR_KEY, WITNESS_COMMIT),
  ];
  for (let seq = 1; seq <= entry.head_seq; seq += 1) {
    events.push({
      event_id: seq === entry.head_seq ? entry.head_event : syntheticEventId(tag, POSITION_MARKERS[seq]),
      seq,
      ledger_id: entry.ledger_id,
      prev: "",
      timestamp_ms: firstSeenMs + seq * 60000,
      author_key: AUTHOR_KEY,
      payload_kind: seq === 1 ? "witness_set" : "trust_attestation",
      payload: seq === 1 ? { witnesses: [REACHABLE_WITNESS] } : { subject: ACME },
    });
  }
  return chained(events);
}

function syntheticEntry(
  tag: string,
  declaredKind: DeclaredKind,
  headSeq: number,
  forkCount: number,
  /** The id to mint the ledger under, when it is not the repeated tag. */
  namedId?: string,
): WitnessLedgerSummary {
  return {
    ledger_id: namedId ?? tag.repeat(26),
    declared_kind: declaredKind,
    head_seq: headSeq,
    head_event: syntheticEventId(tag, "z"),
    event_count: headSeq + 1,
    fork_count: forkCount,
  };
}

/** The ledger the witness recorded a second conflicting entry for. */
export const CONFLICTED_LEDGER = "gh".repeat(26);

/**
 * A ledger one witness holds and this home stores no copy of, which is what a
 * fetch pulls. Bob is seeded as already stored, so the unstored case needs a
 * record of its own.
 */
export const UNSTORED_LEDGER = "cd".repeat(26);

export const SYNTHETIC_ENTRIES: WitnessLedgerSummary[] = [
  syntheticEntry("cd", "organization", 3, 0),
  syntheticEntry("gh", "person", 2, 128),
  syntheticEntry("mn", "agent", 1, 0),
  syntheticEntry("tv", "service", 2, 0),
  // Bob is the foreign identity the fixtures name everywhere and this home
  // holds no ledger for, so a witness holding him is what a fetch can pull.
  syntheticEntry("bo", "person", 3, 0, BOB),
];

/** When the witness first saw a ledger it was not seeded with. */
const FIRST_SEEN_MS = 1700000300000;

/**
 * The first entries of a chain the wallet holds in full, as far as the witness
 * got. The last entry takes the summary's head event id, so the summary and the
 * chain agree about where the copy stops.
 */
function upTo(events: LedgerEvent[], entry: WitnessLedgerSummary): LedgerEvent[] {
  const kept = events.slice(0, entry.head_seq + 1).map((event) => ({ ...event }));
  kept[kept.length - 1].event_id = entry.head_event;
  return chained(kept);
}

/** Every ledger the mock witness holds, unsorted; the store orders by ledger id. */
export function witnessLedgers(): HeldLedger[] {
  const held = (entry: WitnessLedgerSummary, firstSeenMs: number): HeldLedger => ({
    entry: { ...entry },
    firstSeenMs,
    witnesses: [REACHABLE_WITNESS],
    events:
      entry.ledger_id === ALICE
        ? // What the witness holds of Alice stops where the holdings page says:
          // the first four of the nine entries her own wallet has.
          upTo(aliceEvents(), entry)
        : entry.ledger_id === ACME
          ? upTo(acmeEvents(), entry)
          : syntheticEvents(entry, firstSeenMs),
  });
  return [
    ...witnessLedgerEntries.map((entry) => held(entry, alice.created_at_ms)),
    ...SYNTHETIC_ENTRIES.map((entry) => held(entry, FIRST_SEEN_MS)),
  ];
}
