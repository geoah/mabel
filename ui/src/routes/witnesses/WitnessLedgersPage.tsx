import { useMemo } from "react";
import { Link, useParams } from "react-router";

import { listIdentities, listWitnessLedgers } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  factsFromResolved,
  type IdentityCardEntry,
  IdentityCardList,
  IdentityPillScope,
  type PillFacts,
  trustedSubjects,
} from "@/components/identity";
import { Identifier } from "@/components/Identifier";
import { Alert } from "@/components/ui/alert";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { degreesOf, named, useResolvedNames } from "@/hooks/useResolvedNames";
import { usePagedList } from "@/hooks/usePagedList";
import { useResource } from "@/hooks/useResource";

/** How many of a witness's records this screen reads before it says so. */
const LEDGER_CAP = 1024;

/**
 * A witness this node cannot reach right now. It is a fact about the network,
 * not about the witness's holdings, so the panel says so and nothing else.
 */
function Unreachable({ endpointId, message }: { endpointId: string; message: string }) {
  return (
    <Alert variant="destructive" data-testid="witness-unreachable">
      <p className="text-sm">
        Your wallet could not reach this witness. That is about the connection, not about the
        records it keeps.
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
  // The route pages, so the screen reads its pages: a witness holding more than
  // one page of records used to render the first page as the whole answer.
  const page = usePagedList(
    (offset, limit) =>
      listWitnessLedgers(endpointId, { offset, limit }).then((response) => ({
        items: response.ledgers,
        more: response.more,
      })),
    [endpointId],
    { cap: LEDGER_CAP },
  );
  const identities = useResource(listIdentities, []);
  const held = identities.data?.identities ?? [];
  const from = held[0]?.identity_id ?? null;
  const ledgerIds = page.items.map((ledger) => ledger.ledger_id);
  const names = useResolvedNames(ledgerIds, from);

  const entries: IdentityCardEntry[] = page.items.map((ledger) => ({
    facts: factsFromResolved(named(names, ledger.ledger_id), {
      declaredKind: ledger.declared_kind,
      to: `/identities/${ledger.ledger_id}`,
    }),
    markers: (
      <>
        {/* How much of a record this witness holds, which is what the listing is
            about. The position it reaches is not a fact about the identity. */}
        <span data-testid={`identity-card-entries-${ledger.ledger_id}`}>
          {ledger.head_seq + 1} {ledger.head_seq === 0 ? "entry" : "entries"}
        </span>
        {ledger.fork_count > 0 && (
          <span data-testid={`identity-card-fork-count-${ledger.ledger_id}`}>
            {ledger.fork_count} {ledger.fork_count === 1 ? "conflict" : "conflicts"}
          </span>
        )}
      </>
    ),
  }));
  const unreachable = page.error?.reason === "witness_unreachable" ? page.error : null;
  // Each name came from one lookup, so the distance it carries costs nothing
  // more: the pills on this screen fire no request of their own.
  const pills = useMemo<PillFacts>(
    () => ({
      own: new Set(held.map((identity) => identity.identity_id)),
      trusted: trustedSubjects(held),
      degrees: degreesOf(names),
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [identities.data, names],
  );

  return (
    <IdentityPillScope facts={pills}>
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
              Asked when this page loaded. A record missing here may be on another witness.
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {page.loading && <p data-testid="witness-ledgers-loading">loading</p>}
            {unreachable && <Unreachable endpointId={endpointId} message={unreachable.message} />}
            {page.error && !unreachable && (
              <ErrorEnvelopeView error={page.error} testId="witness-ledgers-error" />
            )}
            {page.capped && (
              <p data-testid="witness-ledgers-capped" className="text-sm">
                Showing the first {page.items.length} records. This witness holds more.
              </p>
            )}
            {page.loaded && (
              <IdentityCardList
                entries={entries}
                testId="identity-cards"
                empty="This witness holds no record."
                emptyTestId="witness-ledgers-empty"
              />
            )}
          </CardContent>
        </Card>
      </div>
    </IdentityPillScope>
  );
}
