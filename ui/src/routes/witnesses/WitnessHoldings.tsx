import { useMemo, useState } from "react";

import { listWitnessHoldings } from "@/api/client";
import type { Identity, WitnessSummary } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  factsFromResolved,
  type IdentityCardEntry,
  IdentityCardList,
  IdentityPillScope,
  type PillFacts,
  trustedSubjects,
} from "@/components/identity";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Section } from "@/components/Section";
import { Alert } from "@/components/ui/alert";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { degreesOf, named, useResolvedNames } from "@/hooks/useResolvedNames";
import { usePagedList } from "@/hooks/usePagedList";

/** How many of a witness's records this screen reads before it says so. */
const LEDGER_CAP = 1024;

/** Which of the records a witness holds the list is showing. */
type Holdings = "all" | "ours" | "trusted";

/** The three tabs, in the order they are drawn, widest first. */
const FILTERS: { key: Holdings; label: string; sentence: string }[] = [
  { key: "all", label: "All", sentence: "Every record this witness holds." },
  {
    key: "trusted",
    label: "Trusted",
    sentence: "The people you trust, and the ones your wallet reaches through them.",
  },
  { key: "ours", label: "Yours", sentence: "The records your own identities control." },
];

/**
 * A witness no machine answered for. It is a fact about the network, not about
 * the witness's holdings, so the panel says so and nothing else.
 */
function Unreachable({ message }: { message: string }) {
  return (
    <Alert variant="destructive" data-testid="witness-unreachable">
      <p className="text-sm">
        Your wallet could not reach this witness. That is about the connection, not about the
        records it keeps.
      </p>
      <p data-testid="witness-unreachable-message" className="mt-1 text-xs">
        {message}
      </p>
    </Alert>
  );
}

/**
 * What one witness holds, asked live and drawn as the identity card list, under
 * three tabs: everything it holds, the people you have a reason to trust, and
 * the records your own identities control. Above the tabs are the two facts a
 * witness's card used to carry: who chose it, and whether this node sends
 * records there by default.
 *
 * Nothing here is fetched into this home: a card opens the identity page, which
 * is where fetching is an explicit button.
 */
export function WitnessHoldings({
  witness,
  own,
}: {
  witness: WitnessSummary;
  /** The identities this home signs for, for the filters and the pills. */
  own: Identity[];
}) {
  const identityId = witness.identity_id;
  const [holdings, setHoldings] = useState<Holdings>("all");
  // The route pages, so the screen reads its pages: a witness holding more than
  // one page of records used to render the first page as the whole answer.
  const page = usePagedList(
    (offset, limit) =>
      listWitnessHoldings(identityId, { offset, limit }).then((response) => ({
        items: response.ledgers,
        more: response.more,
      })),
    [identityId],
    { cap: LEDGER_CAP },
  );
  const from = own[0]?.identity_id ?? null;
  const ledgerIds = page.items.map((ledger) => ledger.ledger_id);
  const names = useResolvedNames(ledgerIds, from);
  const unreachable = page.error?.reason === "witness_unreachable" ? page.error : null;
  // Each name came from one lookup, so the distance it carries costs nothing
  // more: the pills on this screen fire no request of their own.
  const pills = useMemo<PillFacts>(
    () => ({
      own: new Set(own.map((identity) => identity.identity_id)),
      trusted: trustedSubjects(own),
      degrees: degreesOf(names),
    }),
    [own, names],
  );

  const shown = page.items.filter((ledger) => {
    if (holdings === "ours") {
      return pills.own.has(ledger.ledger_id);
    }
    if (holdings === "trusted") {
      return (
        pills.trusted.has(ledger.ledger_id) || (pills.degrees.get(ledger.ledger_id) ?? 0) > 0
      );
    }
    return true;
  });
  const entries: IdentityCardEntry[] = shown.map((ledger) => ({
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
  const chosen = FILTERS.find((filter) => filter.key === holdings)!;

  return (
    <IdentityPillScope facts={pills}>
      <Section
        testId="witness-holdings"
        title="What this witness holds"
        description={`${chosen.sentence} Asked when this page loaded, so a record missing here may be on another witness.`}
      >
        <KeyValueTable>
          <KeyValue label="chosen by" testId="witness-chosen-by">
            {witness.named_by.length === 0
              ? "none of your identities"
              : `${witness.named_by.length} of your identities`}
          </KeyValue>
          <KeyValue label="used by default" testId="witness-node-default">
            {witness.is_node_default
              ? "yes, for the identities that chose no witness of their own"
              : "no"}
          </KeyValue>
        </KeyValueTable>
        <Tabs value={holdings} onValueChange={(next) => setHoldings(next as Holdings)}>
          <TabsList data-testid="witness-holdings-filter">
            {FILTERS.map((filter) => (
              <TabsTrigger
                key={filter.key}
                value={filter.key}
                data-testid={`witness-holdings-${filter.key}`}
              >
                {filter.label}
              </TabsTrigger>
            ))}
          </TabsList>
          {/* One panel, drawn for whichever tab is chosen: all three hold the
              same list narrowed, so writing it once keeps them from drifting
              apart, and the reading of the witness is asked for once. */}
          <TabsContent value={holdings} className="space-y-3">
            {page.loading && <p data-testid="witness-holdings-loading">loading</p>}
            {unreachable && <Unreachable message={unreachable.message} />}
            {page.error && !unreachable && (
              <ErrorEnvelopeView error={page.error} testId="witness-holdings-error" />
            )}
            {page.capped && (
              <p data-testid="witness-holdings-capped" className="text-sm">
                Showing the first {page.items.length} records. This witness holds more.
              </p>
            )}
            {page.loaded && (
              <IdentityCardList
                entries={entries}
                testId="identity-cards"
                empty={
                  holdings === "all"
                    ? "This witness holds no record."
                    : "No record it holds matches this."
                }
                emptyTestId="witness-holdings-empty"
              />
            )}
          </TabsContent>
        </Tabs>
      </Section>
    </IdentityPillScope>
  );
}
