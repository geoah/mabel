import { listLedgers } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  bareIdentity,
  factsFromResolved,
  type IdentityCardEntry,
  IdentityCardList,
} from "@/components/identity";
import { Section } from "@/components/Section";
import { usePagedList } from "@/hooks/usePagedList";

import { WITNESS_HOLDINGS_NOTE, WITNESS_READ_ONLY_NOTE } from "./notes";

/** How many records this screen reads before it says it stopped. */
const LEDGER_CAP = 1024;

/**
 * The witness node's own route: what this one witness holds, as the same
 * identity card list the wallet draws (proposal 004). A witness resolves no
 * names, follows no trust links and holds no wallet, so every card is its record
 * id and no card wears a pill.
 */
export function WitnessHome() {
  // One list, read to its end: the route pages, and this screen offers no page
  // control, so it follows `more` up to a cap and says when the cap stopped it.
  const page = usePagedList(
    (offset, limit) =>
      listLedgers({ offset, limit }).then((response) => ({
        items: response.entries,
        more: response.more,
      })),
    [],
    { cap: LEDGER_CAP },
  );
  const entries: IdentityCardEntry[] = page.items.map((entry) => ({
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
    <Section
      testId="witness-ledger-list"
      title="Records"
      descriptionTestId="witness-holdings-note"
      description={WITNESS_HOLDINGS_NOTE}
    >
      <p data-testid="witness-read-only-note" className="text-sm text-muted-foreground">
        {WITNESS_READ_ONLY_NOTE}
      </p>
      {page.loading && <p data-testid="witness-ledger-list-loading">loading</p>}
      {page.error && <ErrorEnvelopeView error={page.error} testId="witness-ledger-list-error" />}
      {page.capped && (
        <p data-testid="witness-ledger-list-capped" className="text-sm">
          Showing the first {page.items.length} records. This witness holds more.
        </p>
      )}
      {page.loaded && (
        <IdentityCardList
          entries={entries}
          testId="identity-cards"
          empty="This witness holds no record."
          emptyTestId="witness-ledger-list-empty"
        />
      )}
    </Section>
  );
}
