import { useMemo } from "react";

import { listIdentities, listWitnesses } from "@/api/client";
import type { ResolvedIdentity, WitnessSummary } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  bareIdentity,
  IdentityInline,
  IdentityListScope,
  resolvedFrom,
} from "@/components/identity";
import { WitnessCard } from "@/components/WitnessCard";
import { Badge } from "@/components/ui/badge";
import { useResource } from "@/hooks/useResource";
import { GraphSyncCard } from "@/routes/wallet/GraphSyncControl";

/**
 * One witness endpoint and where this wallet knows it from: the identities whose
 * folded witness config names it, and whether node.json carries it as a default.
 * The identities are named the way every other screen names one, so the block
 * reads as people rather than as a column of opaque ids.
 */
function KnownWitnessCard({
  witness,
  name,
}: {
  witness: WitnessSummary;
  name: (identityId: string) => ResolvedIdentity;
}) {
  const endpoint = witness.endpoint_id;
  const chose = witness.named_by.map(name);
  return (
    <WitnessCard
      endpointId={endpoint}
      testIdPrefix="witness-card"
      badge={
        witness.is_node_default ? (
          <Badge variant="secondary" data-testid={`witness-card-default-${endpoint}`}>
            this node uses it by default
          </Badge>
        ) : undefined
      }
    >
      <p
        data-testid={`witness-card-named-by-${endpoint}`}
        className="text-xs text-muted-foreground"
      >
        chosen by {witness.named_by.length}{" "}
        {witness.named_by.length === 1 ? "identity" : "identities"} of yours
      </p>
      {chose.length > 0 && (
        <IdentityListScope identities={chose}>
          <span className="mt-1 flex flex-col gap-1">
            {chose.map((identity) => (
              <IdentityInline
                key={identity.identity_id}
                identity={identity}
                testId={`witness-card-chose-${endpoint}-${identity.identity_id}`}
                to={`/identities/${identity.identity_id}`}
                // A card has the width for a whole Mabel ID, so it draws one.
                full
              />
            ))}
          </span>
        </IdentityListScope>
      )}
    </WitnessCard>
  );
}

/**
 * The witness card list (proposal 004). A wallet knows a witness from a ledger
 * that names it or from its own configuration: there is no global directory.
 */
export function WitnessesPage() {
  const witnesses = useResource(listWitnesses, []);
  const identities = useResource(listIdentities, []);
  const held = identities.data?.identities ?? [];
  // Every identity that names a witness is an identity this wallet holds, so no
  // request runs to name one: the list it already loaded carries the names.
  const name = useMemo(() => {
    const table = new Map(held.map((identity) => [identity.identity_id, resolvedFrom(identity)]));
    return (identityId: string) => table.get(identityId) ?? bareIdentity(identityId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [identities.data]);

  return (
    <div className="space-y-4">
      {/* Flat sections, like the wallet page: the cards here are the witnesses,
          so no card wraps them. */}
      <section data-testid="witness-list" className="space-y-3">
        <h2 className="text-sm font-semibold tracking-tight">Witnesses</h2>
        <p className="text-xs text-muted-foreground">
          The ones your identities chose, and the ones this node uses by default.
        </p>
        {witnesses.loading && <p data-testid="witness-list-loading">loading</p>}
        {witnesses.error && (
          <ErrorEnvelopeView error={witnesses.error} testId="witness-list-error" />
        )}
        {witnesses.data && witnesses.data.witnesses.length === 0 && (
          <p data-testid="witness-list-empty">Your wallet knows of no witness yet.</p>
        )}
        {witnesses.data && witnesses.data.witnesses.length > 0 && (
          <ul data-testid="witness-cards" className="grid gap-2">
            {witnesses.data.witnesses.map((witness) => (
              <li key={witness.endpoint_id} className="min-w-0">
                <KnownWitnessCard witness={witness} name={name} />
              </li>
            ))}
          </ul>
        )}
      </section>
      {/* The sync reads witnesses, so the control that starts one lives here. */}
      <GraphSyncCard />
    </div>
  );
}
