import { listLedgers } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { type IdentityCardEntry, IdentityCardList } from "@/components/IdentityCardList";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { bareIdentity } from "@/hooks/useResolvedNames";
import { useResource } from "@/hooks/useResource";

import { WITNESS_HOLDINGS_NOTE, WITNESS_READ_ONLY_NOTE } from "./notes";

/**
 * The witness node's own debug route: what this one witness holds, as the same
 * identity card list the wallet draws (proposal 004). A witness resolves no
 * names and runs no crawl, so every card is its ledger id.
 */
export function WitnessHome() {
  const page = useResource(() => listLedgers({ limit: 256 }), []);
  const entries: IdentityCardEntry[] = (page.data?.entries ?? []).map((entry) => ({
    identity: bareIdentity(entry.ledger_id),
    declaredKind: entry.declared_kind,
    headSeq: entry.head_seq,
    to: `/witness/ledgers/${entry.ledger_id}`,
    markers:
      entry.fork_count > 0 ? (
        <span data-testid={`identity-card-fork-count-${entry.ledger_id}`}>
          {entry.fork_count} fork {entry.fork_count === 1 ? "record" : "records"}
          {entry.forks_truncated ? ", recording stopped" : ""}
        </span>
      ) : null,
  }));

  return (
    <Card data-testid="witness-ledger-list">
      <CardHeader>
        <CardTitle>Ledgers</CardTitle>
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
            empty="this witness holds no ledger"
            emptyTestId="witness-ledger-list-empty"
          />
        )}
      </CardContent>
    </Card>
  );
}
