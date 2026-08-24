import { listLedgers } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  bareIdentity,
  factsFromResolved,
  type IdentityCardEntry,
  IdentityCardList,
} from "@/components/identity";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

import { WITNESS_HOLDINGS_NOTE, WITNESS_READ_ONLY_NOTE } from "./notes";

/**
 * The witness node's own route: what this one witness holds, as the same
 * identity card list the wallet draws (proposal 004). A witness resolves no
 * names, follows no trust links and holds no wallet, so every card is its record
 * id and no card wears a pill.
 */
export function WitnessHome() {
  const page = useResource(() => listLedgers({ limit: 256 }), []);
  const entries: IdentityCardEntry[] = (page.data?.entries ?? []).map((entry) => ({
    facts: factsFromResolved(bareIdentity(entry.ledger_id), {
      declaredKind: entry.declared_kind,
      to: `/witness/ledgers/${entry.ledger_id}`,
    }),
    markers: (
      <>
        <span data-testid={`identity-card-entries-${entry.ledger_id}`}>
          {entry.head_seq + 1} {entry.head_seq === 0 ? "entry" : "entries"}
        </span>
        {entry.fork_count > 0 && (
          <span data-testid={`identity-card-fork-count-${entry.ledger_id}`}>
            {entry.fork_count} {entry.fork_count === 1 ? "conflict" : "conflicts"}
            {entry.forks_truncated ? ", and it stopped recording more" : ""}
          </span>
        )}
      </>
    ),
  }));

  return (
    <Card data-testid="witness-ledger-list">
      <CardHeader>
        <CardTitle>Records</CardTitle>
        <CardDescription data-testid="witness-holdings-note">
          {WITNESS_HOLDINGS_NOTE}
        </CardDescription>
        <CardDescription data-testid="witness-read-only-note">
          {WITNESS_READ_ONLY_NOTE}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {page.loading && <p data-testid="witness-ledger-list-loading">loading</p>}
        {page.error && (
          <ErrorEnvelopeView error={page.error} testId="witness-ledger-list-error" />
        )}
        {page.data && (
          <IdentityCardList
            entries={entries}
            testId="identity-cards"
            empty="This witness holds no record."
            emptyTestId="witness-ledger-list-empty"
          />
        )}
      </CardContent>
    </Card>
  );
}
