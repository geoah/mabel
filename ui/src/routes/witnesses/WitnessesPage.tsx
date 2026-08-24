import { Link } from "react-router";

import { listWitnesses } from "@/api/client";
import type { WitnessSummary } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";
import { GraphSyncCard } from "@/routes/wallet/GraphSyncControl";

/**
 * One witness endpoint and where this wallet knows it from: the ledgers whose
 * folded witness config names it, and whether node.json carries it as a default.
 */
function WitnessCard({ witness }: { witness: WitnessSummary }) {
  const endpoint = witness.endpoint_id;
  return (
    <Card
      data-testid={`witness-card-${endpoint}`}
      className="overflow-hidden transition-colors hover:border-foreground/30 hover:bg-accent"
    >
      <Link
        to={`/witnesses/${endpoint}`}
        data-testid={`witness-card-link-${endpoint}`}
        className="flex min-h-16 flex-col justify-center gap-1.5 p-3 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring sm:p-4"
      >
        <Identifier value={endpoint} plain />
        <span className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
          <span data-testid={`witness-card-named-by-${endpoint}`}>
            chosen by {witness.named_by.length}{" "}
            {witness.named_by.length === 1 ? "identity" : "identities"} of yours
          </span>
          {witness.is_node_default && (
            <Badge variant="secondary" data-testid={`witness-card-default-${endpoint}`}>
              this node uses it by default
            </Badge>
          )}
        </span>
        {witness.named_by.length > 0 && (
          <span className="flex flex-col gap-0.5">
            {witness.named_by.map((identityId) => (
              <Identifier key={identityId} value={identityId} plain />
            ))}
          </span>
        )}
      </Link>
    </Card>
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
                  <WitnessCard witness={witness} />
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
