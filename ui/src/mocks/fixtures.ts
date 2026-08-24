// The frozen fixtures, imported straight from contracts/http and contracts/cli
// so the contract stays the single source of truth. Nothing here is edited: the
// casts only narrow the structural types TypeScript infers from JSON onto the
// hand-written contract types in src/api/types.ts.

import walletNodeFixture from "@contracts/http/wallet-get-node.json";
import identitiesFixture from "@contracts/http/wallet-get-identities.json";
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
import witnessLedgersFixture from "@contracts/http/wallet-get-witness-ledgers.json";
import resolveFixture from "@contracts/http/wallet-get-resolve.json";
import fetchFixture from "@contracts/http/wallet-post-identity-fetch.json";
import witnessNodeFixture from "@contracts/http/witness-get-node.json";
import ledgerEventsFixture from "@contracts/http/witness-get-ledger-events.json";
import ledgersFixture from "@contracts/http/witness-get-ledgers.json";
import ledgerEntryFixture from "@contracts/http/witness-get-ledger.json";
import forksFixture from "@contracts/http/witness-get-forks.json";
import cliErrors from "@contracts/cli/errors.json";

import type {
  AppendResponse,
  Contact,
  CreateIdentityResponse,
  DeclaredKind,
  ErrorEnvelope,
  ForkRecord,
  Graph,
  Identity,
  IdentityKeysResponse,
  LedgerEvent,
  LedgerPageResponse,
  LedgerSummary,
  LookupResponse,
  ReplaceProfileResponse,
  ResolvedIdentity,
  RevokeTrustResponse,
  SyncPushResponse,
  Verification,
  WalletNodeInfo,
  WitnessNodeInfo,
  WitnessSummary,
} from "@/api/types";

export const walletNode = walletNodeFixture.response as WalletNodeInfo;
export const witnessNode = witnessNodeFixture.response as WitnessNodeInfo;
export const seedIdentities = identitiesFixture.response.identities as Identity[];
export const createdIdentity = createIdentityFixture.response as CreateIdentityResponse;
export const witnessConfigAppend = witnessesFixture.response as AppendResponse;
export const trustAppend = trustFixture.response as AppendResponse;
export const trustRevoke = revokeFixture.response as RevokeTrustResponse;
export const syncPush = syncPushFixture.response as SyncPushResponse;
export const seedLedgerEvents = ledgerEventsFixture.response as LedgerPageResponse;

/** GET /api/witnesses, the three endpoints the frozen answer names. */
export const knownWitnesses = knownWitnessesFixture.response.witnesses as WitnessSummary[];
/**
 * The endpoint the frozen witness list carries that no ledger of this home
 * names and node.json does not default to. The mock hands it to acme so both
 * the "node default" marker and the unreachable drill-in have a case.
 */
export const UNREACHABLE_WITNESS = knownWitnesses.find(
  (witness) => !witness.is_node_default,
)!.endpoint_id;
/** The reachable witness the drill-in answers for, a node default. */
export const REACHABLE_WITNESS = witnessLedgersFixture.response.endpoint_id;
/** GET /api/witnesses/:endpoint_id/ledgers, unreachable: 502 witness_unreachable. */
export const witnessUnreachableError = witnessLedgersFixture.errors[2] as {
  status: number;
  body: ErrorEnvelope;
};
/** GET /api/resolve/:hostname, the hostname alice's profile claims. */
export const resolvedHostname = resolveFixture.response as {
  ok: true;
  hostname: string;
  identity_id: string;
  status: string;
};
/** POST /api/identities/:identity_id/fetch, refused because no source holds it. */
export const fetchNotHeldError = fetchFixture.errors[1] as {
  status: number;
  body: ErrorEnvelope;
};

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
export const noKeysHeldError = identityKeysFixture.errors[1] as {
  status: number;
  body: ErrorEnvelope;
};

/** GET /api/lookup/:identity_id, the two-hop answer from Alice to Carol. */
export const seedLookup = lookupFixture.response as LookupResponse;
/** GET /api/graph, the crawl generation this home last recorded. */
export const seedGraph = graphFixture.response.graph as Graph;

/** The ids the fixtures share with test-vectors/. */
export const ALICE = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";
export const BOB = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";
export const ACME = "2okqwhextnpkpmydrgrkk563vbehcklffwfzidxlh5dslawjmn6a";
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

/** Named error bodies the handlers and the tests reuse, one per exit-code class. */
export const errors = {
  /** code 2, no layer prefix. */
  missingField: createIdentityFixture.errors[0] as { status: number; body: ErrorEnvelope },
  /** code 10, Schema error. */
  unknownEnumValue: createIdentityFixture.errors[1] as { status: number; body: ErrorEnvelope },
  /** code 70, no layer prefix. */
  unsupportedDeclaredKind: createIdentityFixture.errors[4] as {
    status: number;
    body: ErrorEnvelope;
  },
  /** code 20, Policy error. */
  duplicateAttestation: trustFixture.errors[1] as { status: number; body: ErrorEnvelope },
  /** code 2, an identity this node home holds no ledger for. */
  unknownLedger: trustFixture.errors[2] as { status: number; body: ErrorEnvelope },
  /** code 30, Network error. */
  allWitnessesFailed: syncPushFixture.errors[0] as { status: number; body: ErrorEnvelope },
  /** code 50, State error. */
  staleHead: witnessesFixture.errors[1] as { status: number; body: ErrorEnvelope },
  /** code 60, insecure key permissions, no layer prefix. */
  insecureKeyPermissions: identitiesFixture.errors[1] as {
    status: number;
    body: ErrorEnvelope;
  },
  /** code 2, a profile body that names only one of the two keys. */
  profileMissingField: profileFixture.errors[0] as { status: number; body: ErrorEnvelope },
  /** code 20 at 409, a replacement that would change nothing. */
  noOpProfileUpdate: profileFixture.errors[1] as { status: number; body: ErrorEnvelope },
  /** code 10, a display name that parses as an identity id. */
  invalidDisplayName: profileFixture.errors[2] as { status: number; body: ErrorEnvelope },
  /** code 20 at 409, a forced check on an identity claiming no hostname. */
  noHostnameClaimed: verificationFixture.errors[0] as { status: number; body: ErrorEnvelope },
  /** code 10, a contact nickname past its 64-byte cap. */
  contactFieldTooLong: contactPutFixture.errors[0] as { status: number; body: ErrorEnvelope },
  /** code 2, a lookup whose from names no identity in this home. */
  unknownFromIdentity: lookupFixture.errors[1] as { status: number; body: ErrorEnvelope },
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

/**
 * The four entries the frozen page carries for Alice, which is what the witness
 * holds: its own summary of her record stops at position 3.
 */
export function seedEvents(): LedgerEvent[] {
  return seedLedgerEvents.events.map((event) => ({ ...event }));
}

/**
 * Alice's chain as her own wallet holds it: the four frozen entries plus the
 * five the identity document implies. Her document reports nine entries and a
 * head at 8, so a chain of four would render four entries against a head of
 * eight, which is the thing decision 017 refuses to show.
 *
 * The added entries are the ones the folded state demands: her profile lands at
 * position 7, where the document says its event is, and the attestation naming
 * Bob lands at 8, where the document's head event is.
 */
export function aliceEvents(): LedgerEvent[] {
  const alice = seedIdentities.find((identity) => identity.identity_id === ALICE)!;
  const trusted = alice.trust.find((record) => !record.revoked)!;
  const profile = alice.profile!;
  const frozen = seedEvents();
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
    author_key: String(alice.active_key),
    payload_kind: payloadKind,
    payload,
  });

  const events = [
    ...frozen,
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
  ];
  for (const event of events.slice(frozen.length)) {
    event.prev = events[event.seq - 1].event_id;
  }
  return events;
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
  const acme = seedIdentities.find((identity) => identity.identity_id === ACME)!;
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

  const events = [
    // Entry 0 is the inception, and its event id is the identity id.
    shape(0, ACME, "inception", {
      declared_kind: acme.declared_kind,
      nonce: INCEPTION_NONCE,
      root: {
        identity_root: {
          founder: founder.identity,
          founder_key: founder.active_key,
          founder_inception: seedLedgerEvents.events[0].event_id,
        },
      },
    }),
    shape(1, syntheticEventId("ac", "b"), "witness_config", {
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
  ];
  for (const event of events.slice(1)) {
    event.prev = events[event.seq - 1].event_id;
  }
  return events;
}

// The witness side: what one witness holds. The two frozen entries page in one
// request, so the mock adds four more ledgers to give the list a second page,
// and one of them stopped recording forks (forks_truncated).

export const witnessLedgerEntries = ledgersFixture.response.entries as LedgerSummary[];
export const witnessLedgerWitnesses = ledgerEntryFixture.response.witnesses as string[];
export const witnessForkFixture = forksFixture.response.entries[0] as ForkRecord;

/** One ledger a witness holds: the summary it serves, its witnesses and its chain. */
export interface HeldLedger {
  entry: LedgerSummary;
  witnesses: string[];
  events: LedgerEvent[];
}

/** The base32 id of a minted event, distinct per ledger and per position. */
function syntheticEventId(tag: string, marker: string): string {
  return `${tag}${marker}`.repeat(18).slice(0, 52);
}

const POSITION_MARKERS = "abcdefghijklmnop";
const AUTHOR_KEY = seedLedgerEvents.events[0].author_key;

/** The frozen seq-0 payload, so a synthetic inception cannot drift from it. */
const FROZEN_INCEPTION = seedLedgerEvents.events[0].payload as {
  nonce: string;
  root: { raw_root: { active_key: string; reserve_commit: string } };
};
const INCEPTION_NONCE = FROZEN_INCEPTION.nonce;
const RESERVE_COMMIT = FROZEN_INCEPTION.root.raw_root.reserve_commit;

/**
 * A chain shaped like the frozen one for a ledger the fixtures do not carry:
 * seq 0 is the inception whose event id is the ledger id, and the last event id
 * is the summary's head_event, so the summary and the chain agree.
 */
function syntheticEvents(entry: LedgerSummary): LedgerEvent[] {
  const tag = entry.ledger_id.slice(0, 2);
  const events: LedgerEvent[] = [];
  for (let seq = 0; seq <= entry.head_seq; seq += 1) {
    const eventId =
      seq === 0
        ? entry.ledger_id
        : seq === entry.head_seq
          ? entry.head_event
          : syntheticEventId(tag, POSITION_MARKERS[seq]);
    events.push({
      event_id: eventId,
      seq,
      ledger_id: seq === 0 ? null : entry.ledger_id,
      prev: seq === 0 ? null : events[seq - 1].event_id,
      timestamp_ms: entry.first_seen_ms + seq * 60000,
      author_key: AUTHOR_KEY,
      // One inception payload_kind for both roots (contracts/README.md, "Event
      // document"); these synthetic ledgers all carry a raw root.
      payload_kind:
        seq === 0 ? "inception" : seq === 1 ? "witness_config" : "trust_attestation",
      payload:
        seq === 0
          ? {
              declared_kind: entry.declared_kind,
              nonce: INCEPTION_NONCE,
              root: { raw_root: { active_key: AUTHOR_KEY, reserve_commit: RESERVE_COMMIT } },
            }
          : seq === 1
            ? { witnesses: [witnessLedgerWitnesses[0]] }
            : { subject: ACME },
    });
  }
  return events;
}

function syntheticEntry(
  tag: string,
  declaredKind: DeclaredKind,
  headSeq: number,
  forkCount: number,
  forksTruncated: boolean,
  /** The id to mint the ledger under, when it is not the repeated tag. */
  namedId?: string,
): LedgerSummary {
  const ledgerId = namedId ?? tag.repeat(26);
  const firstSeen = 1700000300000;
  return {
    ledger_id: ledgerId,
    declared_kind: declaredKind,
    head_seq: headSeq,
    head_event: syntheticEventId(tag, "z"),
    event_count: headSeq + 1,
    first_seen_ms: firstSeen,
    updated_ms: firstSeen + headSeq * 60000,
    fork_count: forkCount,
    forks_truncated: forksTruncated,
    source_endpoint: witnessLedgerWitnesses[0],
  };
}

/** The ledger the witness stopped recording forks for, flagged forks_truncated. */
export const TRUNCATED_LEDGER = "gh".repeat(26);

export const SYNTHETIC_ENTRIES: LedgerSummary[] = [
  syntheticEntry("cd", "organization", 3, 0, false),
  syntheticEntry("gh", "person", 2, 128, true),
  syntheticEntry("mn", "agent", 1, 0, false),
  syntheticEntry("tv", "service", 2, 0, false),
  // Bob is the foreign identity the fixtures name everywhere and this home
  // holds no ledger for, so a witness holding him is what a fetch can pull.
  syntheticEntry("bo", "person", 3, 0, false, BOB),
];

/**
 * The fork statement, worded by the node and reproduced here from the frozen
 * fixture so the mock cannot drift from the contract's wording.
 */
function forkStatement(ledgerId: string, seq: number): string {
  return witnessForkFixture.statement
    .replaceAll(ALICE, ledgerId)
    .replace(`seq ${witnessForkFixture.seq}`, `seq ${seq}`);
}

/** Every ledger the mock witness holds, unsorted; the store orders by ledger id. */
export function witnessLedgers(): HeldLedger[] {
  const frozen = witnessLedgerEntries.map((entry) => ({
    entry: { ...entry },
    witnesses: [...witnessLedgerWitnesses],
    events: entry.ledger_id === ALICE ? seedEvents() : syntheticEvents(entry),
  }));
  const minted = SYNTHETIC_ENTRIES.map((entry) => ({
    entry: { ...entry },
    witnesses: [witnessLedgerWitnesses[0]],
    events: syntheticEvents(entry),
  }));
  return [...frozen, ...minted];
}

/** Alice's frozen fork record plus one on the ledger whose fork list is truncated. */
export function witnessForks(): ForkRecord[] {
  const truncated = SYNTHETIC_ENTRIES.find((entry) => entry.ledger_id === TRUNCATED_LEDGER)!;
  const chain = syntheticEvents(truncated);
  const kept = chain[1];
  return [
    { ...witnessForkFixture },
    {
      ledger_id: truncated.ledger_id,
      seq: kept.seq,
      observed_ms: truncated.updated_ms,
      source_endpoint: witnessForkFixture.source_endpoint,
      kept,
      conflicting: {
        ...kept,
        event_id: syntheticEventId("gh", "q"),
        timestamp_ms: kept.timestamp_ms + 10000,
        payload_kind: "trust_attestation",
        payload: { subject: BOB },
      },
      statement: forkStatement(truncated.ledger_id, kept.seq),
    },
  ];
}
