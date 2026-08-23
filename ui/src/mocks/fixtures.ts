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
import lookupFixture from "@contracts/http/wallet-get-lookup.json";
import graphFixture from "@contracts/http/wallet-get-graph.json";
import createIdentityFixture from "@contracts/http/wallet-post-identities.json";
import witnessesFixture from "@contracts/http/wallet-post-identity-witnesses.json";
import trustFixture from "@contracts/http/wallet-post-trust.json";
import revokeFixture from "@contracts/http/wallet-post-trust-revoke.json";
import syncPushFixture from "@contracts/http/wallet-post-sync-push.json";
import verifyFixture from "@contracts/http/wallet-post-verify.json";
import witnessNodeFixture from "@contracts/http/witness-get-node.json";
import ledgerEventsFixture from "@contracts/http/witness-get-ledger-events.json";
import ledgersFixture from "@contracts/http/witness-get-ledgers.json";
import ledgerEntryFixture from "@contracts/http/witness-get-ledger.json";
import forksFixture from "@contracts/http/witness-get-forks.json";
import verifyTrustCases from "@contracts/cli/verify-trust.json";
import verifyLedgerCases from "@contracts/cli/verify-ledger.json";
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
  LedgerEvent,
  LedgerPageResponse,
  LedgerSummary,
  LookupResponse,
  ReplaceProfileResponse,
  ResolvedIdentity,
  RevokeTrustResponse,
  SyncPushResponse,
  Verification,
  VerifyLedgerReport,
  VerifyTrustReport,
  WalletNodeInfo,
  WitnessNodeInfo,
} from "@/api/types";

export const walletNode = walletNodeFixture.response as WalletNodeInfo;
export const witnessNode = witnessNodeFixture.response as WitnessNodeInfo;
export const seedIdentities = identitiesFixture.response.identities as Identity[];
export const createdIdentity = createIdentityFixture.response as CreateIdentityResponse;
export const witnessConfigAppend = witnessesFixture.response as AppendResponse;
export const trustAppend = trustFixture.response as AppendResponse;
export const trustRevoke = revokeFixture.response as RevokeTrustResponse;
export const syncPush = syncPushFixture.response as SyncPushResponse;
export const verifyTrust = verifyFixture.response as VerifyTrustReport;
export const seedLedgerEvents = ledgerEventsFixture.response as LedgerPageResponse;

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

function verifyTrustCase(name: string): VerifyTrustReport {
  const found = verifyTrustCases.cases.find((entry) => entry.case === name);
  if (!found) {
    throw new Error(`no verify trust fixture case named ${name}`);
  }
  return found.document as VerifyTrustReport;
}

/** contracts/cli/verify-trust.json, one unrevoked attestation in 0..=head. */
export const verifyTrustTrusted = verifyTrustCase("trusted");
/** contracts/cli/verify-trust.json, every attestation revoked. */
export const verifyTrustRevoked = verifyTrustCase("not-trusted-because-revoked");
/** contracts/cli/verify-trust.json, no queried source holds the subject. */
export const verifyTrustUnresolved = verifyTrustCase("unresolved-subject");

export const verifyLedgerValid = verifyLedgerCases.cases[0].document as VerifyLedgerReport;
/** Partial validity is a failure: exit 20 with the report fields under details. */
export const verifyLedgerPartial = verifyLedgerCases.cases[1].document as ErrorEnvelope;

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
  /** code 2, identity not in this node home. */
  identityNotFound: trustFixture.errors[2] as { status: number; body: ErrorEnvelope },
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

/** Alice's four-event chain, the only seeded ledger. */
export function seedEvents(): LedgerEvent[] {
  return seedLedgerEvents.events.map((event) => ({ ...event }));
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
): LedgerSummary {
  const ledgerId = tag.repeat(26);
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
