// The frozen fixtures, imported straight from contracts/http and contracts/cli
// so the contract stays the single source of truth. Nothing here is edited: the
// casts only narrow the structural types TypeScript infers from JSON onto the
// hand-written contract types in src/api/types.ts.

import walletNodeFixture from "@contracts/http/wallet-get-node.json";
import identitiesFixture from "@contracts/http/wallet-get-identities.json";
import createIdentityFixture from "@contracts/http/wallet-post-identities.json";
import witnessesFixture from "@contracts/http/wallet-post-identity-witnesses.json";
import trustFixture from "@contracts/http/wallet-post-trust.json";
import revokeFixture from "@contracts/http/wallet-post-trust-revoke.json";
import syncPushFixture from "@contracts/http/wallet-post-sync-push.json";
import verifyFixture from "@contracts/http/wallet-post-verify.json";
import witnessNodeFixture from "@contracts/http/witness-get-node.json";
import ledgerEventsFixture from "@contracts/http/witness-get-ledger-events.json";
import verifyTrustCases from "@contracts/cli/verify-trust.json";
import verifyLedgerCases from "@contracts/cli/verify-ledger.json";
import cliErrors from "@contracts/cli/errors.json";

import type {
  AppendResponse,
  CreateIdentityResponse,
  ErrorEnvelope,
  Identity,
  LedgerEvent,
  LedgerPageResponse,
  RevokeTrustResponse,
  SyncPushResponse,
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

/** The ids the fixtures share with test-vectors/. */
export const ALICE = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";
export const BOB = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";
export const ACME = "2okqwhextnpkpmydrgrkk563vbehcklffwfzidxlh5dslawjmn6a";

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
