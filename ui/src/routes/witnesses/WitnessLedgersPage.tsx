import { Link, useParams } from "react-router";

import { listIdentities, listWitnessLedgers } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { type IdentityCardEntry, IdentityCardList } from "@/components/IdentityCardList";
import { Identifier } from "@/components/Identifier";
import { Alert } from "@/components/ui/alert";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { bareIdentity, useResolvedNames } from "@/hooks/useResolvedNames";
import { useResource } from "@/hooks/useResource";

/**
 * A witness this node cannot reach right now. It is a fact about the network,
 * not about the witness's holdings, so the panel says so and nothing else.
 */
function Unreachable({ endpointId, message }: { endpointId: string; message: string }) {
  return (
    <Alert variant="destructive" data-testid="witness-unreachable">
      <p className="text-sm">
        this node could not reach the witness while asking what it holds. That is a fact about
        the connection, not about the ledgers it keeps.
      </p>
      <p className="mt-1 text-xs">
        <Identifier value={endpointId} />
      </p>
      <p data-testid="witness-unreachable-message" className="mt-1 text-xs">
        {message}
      </p>
    </Alert>
  );
}

/**
 * What one witness holds, asked live and rendered as the identity card list
 * (proposal 004). Nothing here is fetched into this home: a card opens the
 * identity page, which is where fetching is an explicit button.
 */
export function WitnessLedgersPage() {
  const { endpointId = "" } = useParams();
  const page = useResource(() => listWitnessLedgers(endpointId), [endpointId]);
  const identities = useResource(listIdentities, []);
  const from = identities.data?.identities[0]?.identity_id ?? null;
  const ledgerIds = (page.data?.ledgers ?? []).map((ledger) => ledger.ledger_id);
  const names = useResolvedNames(ledgerIds, from);

  const entries: IdentityCardEntry[] = (page.data?.ledgers ?? []).map((ledger) => ({
    identity: names.get(ledger.ledger_id) ?? bareIdentity(ledger.ledger_id),
    declaredKind: ledger.declared_kind,
    headSeq: ledger.head_seq,
    to: `/identities/${ledger.ledger_id}`,
    markers:
      ledger.fork_count > 0 ? (
        <span data-testid={`identity-card-fork-count-${ledger.ledger_id}`}>
          {ledger.fork_count} fork {ledger.fork_count === 1 ? "record" : "records"}
        </span>
      ) : null,
  }));
  const unreachable = page.error?.reason === "witness_unreachable" ? page.error : null;

  return (
    <div className="space-y-4">
      <Link
        to="/witnesses"
        className="inline-flex min-h-10 items-center text-sm underline"
        data-testid="witness-ledgers-back"
      >
        Witnesses
      </Link>
      <Card data-testid="witness-ledgers">
        <CardHeader>
          <CardTitle className="flex flex-wrap items-baseline gap-2">
            What this witness holds
            <Identifier value={endpointId} />
          </CardTitle>
          <CardDescription>
            Asked over the sync protocol as this page loaded, and stored nowhere: a ledger missing
            here may still exist on another witness
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          {page.loading && <p data-testid="witness-ledgers-loading">loading</p>}
          {unreachable && (
            <Unreachable endpointId={endpointId} message={unreachable.message} />
          )}
          {page.error && !unreachable && (
            <ErrorEnvelopeView error={page.error} testId="witness-ledgers-error" />
          )}
          {page.data && (
            <IdentityCardList
              entries={entries}
              testId="identity-cards"
              empty="this witness holds no ledger"
              emptyTestId="witness-ledgers-empty"
            />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
