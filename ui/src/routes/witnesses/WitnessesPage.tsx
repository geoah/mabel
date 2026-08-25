import { useMemo } from "react";

import { listIdentities, listWitnesses } from "@/api/client";
import type { WitnessSummary } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  bareIdentity,
  factsFromResolved,
  type IdentityCardEntry,
  IdentityCardList,
  IdentityPillScope,
  machinesOf,
  type PillFacts,
  trustedSubjects,
} from "@/components/identity";
import { PageSections, Section } from "@/components/Section";
import { useResource } from "@/hooks/useResource";
import { GraphSyncCard } from "@/routes/wallet/GraphSyncControl";

/** What a witness card says beyond the name every identity card carries. */
export const NODE_DEFAULT_MARKER = "this node uses it by default";

/** The card one witness draws: an identity card, because a witness is an identity. */
function witnessEntry(witness: WitnessSummary): IdentityCardEntry {
  const named = {
    ...bareIdentity(witness.identity_id),
    display_name: witness.display_name,
  };
  return {
    facts: factsFromResolved(named, {
      to: `/identities/${witness.identity_id}`,
      stored: witness.stored,
      machines: machinesOf(null, witness),
    }),
    markers: witness.is_node_default ? (
      <span data-testid={`witness-default-${witness.identity_id}`}>{NODE_DEFAULT_MARKER}</span>
    ) : undefined,
  };
}

/**
 * The witnesses this home knows, each drawn as the identity card every other
 * screen draws (proposal 006 section 8). A home knows a witness from a ledger
 * that names it or from its own configuration: there is no global directory.
 * What one of them holds, and which endpoints answer for it, are on its own page.
 */
export function WitnessesPage() {
  const witnesses = useResource(listWitnesses, []);
  const identities = useResource(listIdentities, []);
  const held = identities.data?.identities ?? [];
  const rows = witnesses.data?.witnesses ?? [];
  const entries = rows.map(witnessEntry);
  // The pills read documents this page already loaded, so no request here
  // exists for the sake of a pill (proposal 005).
  const pills = useMemo<PillFacts>(
    () => ({
      own: new Set(held.map((identity) => identity.identity_id)),
      trusted: trustedSubjects(held),
      degrees: new Map<string, number>(),
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [identities.data],
  );

  return (
    <IdentityPillScope facts={pills}>
      <PageSections>
        {/* Flat sections, like the wallet page: the cards here are the
            witnesses, so no card wraps them. */}
        <Section
          testId="witness-list"
          title="Witnesses"
          description="The ones your identities chose, and the ones this node uses by default."
        >
          {witnesses.loading && <p data-testid="witness-list-loading">loading</p>}
          {witnesses.error && (
            <ErrorEnvelopeView error={witnesses.error} testId="witness-list-error" />
          )}
          {witnesses.data && (
            <IdentityCardList
              entries={entries}
              testId="witness-cards"
              empty="Your wallet knows of no witness yet."
              emptyTestId="witness-list-empty"
            />
          )}
        </Section>
        {/* The sync reads witnesses, so the control that starts one lives here. */}
        <GraphSyncCard />
      </PageSections>
    </IdentityPillScope>
  );
}
