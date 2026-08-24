import { listWitnesses } from "@/api/client";
import type { WitnessSummary } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { WitnessCard } from "@/components/WitnessCard";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";
import { GraphSyncCard } from "@/routes/wallet/GraphSyncControl";

/**
 * One witness endpoint and where this wallet knows it from: the ledgers whose
 * folded witness config names it, and whether node.json carries it as a default.
 */
function KnownWitnessCard({ witness }: { witness: WitnessSummary }) {
  const endpoint = witness.endpoint_id;
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
      {witness.named_by.length > 0 && (
        <span className="flex flex-col gap-0.5">
          {witness.named_by.map((identityId) => (
            <Identifier key={identityId} value={identityId} plain />
          ))}
        </span>
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

  return (
    <div className="space-y-4">
      <Card data-testid="witness-list">
        <CardHeader>
          <CardTitle>Witnesses</CardTitle>
          <CardDescription>
            A witness keeps a copy of an identity&apos;s record so other people can read it. These
            are the ones your wallet knows: the ones your identities chose, and the ones this node
            uses by default.
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
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
                  <KnownWitnessCard witness={witness} />
                </li>
              ))}
            </ul>
          )}
        </CardContent>
      </Card>
      {/* The sync reads witnesses, so the control that starts one lives here. */}
      <GraphSyncCard />
    </div>
  );
}
