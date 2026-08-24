import { useMemo, useState } from "react";
import { useParams } from "react-router";

import { listIdentities, listWitnesses, listWitnessLedgers } from "@/api/client";
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
import { PageSections, Section } from "@/components/Section";
import { Alert } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { degreesOf, named, useResolvedNames } from "@/hooks/useResolvedNames";
import { usePagedList } from "@/hooks/usePagedList";
import { useResource } from "@/hooks/useResource";

/** How many of a witness's records this screen reads before it says so. */
const LEDGER_CAP = 1024;

/** Which of the records a witness holds the list is showing. */
type Holdings = "all" | "ours" | "trusted";

/** The three buttons, in the order they narrow the list. */
const FILTERS: { key: Holdings; label: string; sentence: string }[] = [
  { key: "all", label: "All", sentence: "Every record this witness holds." },
  { key: "ours", label: "Yours", sentence: "The records your own identities control." },
  {
    key: "trusted",
    label: "Trusted",
    sentence: "The people you trust, and the ones your wallet reaches through them.",
  },
];

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
      <p className="mt-1">
        <Identifier value={endpointId} copyLabel="Copy Iroh ID" />
      </p>
      <p data-testid="witness-unreachable-message" className="mt-1 text-xs">
        {message}
      </p>
    </Alert>
  );
}

/**
 * What one witness holds, asked live and rendered as the identity card list
 * (proposal 004), with three ways to narrow it: everything it holds, the records
 * your own identities control, and the people you have a reason to trust.
 *
 * Nothing here is fetched into this home: a card opens the identity page, which
 * is where fetching is an explicit button. The way back to the list of witnesses
 * is the nav, so this page draws no back link of its own.
 */
export function WitnessLedgersPage() {
  const { endpointId = "" } = useParams();
  const [holdings, setHoldings] = useState<Holdings>("all");
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
  const witnesses = useResource(listWitnesses, []);
  const held = identities.data?.identities ?? [];
  const from = held[0]?.identity_id ?? null;
  const ledgerIds = page.items.map((ledger) => ledger.ledger_id);
  const names = useResolvedNames(ledgerIds, from);
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
  // Where this wallet knows the witness from, which is a sentence about the
  // witness and belongs on its own page rather than on its card in a list.
  const summary = witnesses.data?.witnesses.find(
    (witness) => witness.endpoint_id === endpointId,
  );
  const chosenBy = summary?.named_by.length ?? 0;

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
      <PageSections>
        <div className="space-y-2">
          <h1 className="text-2xl leading-tight font-semibold tracking-tight">This witness</h1>
          <Identifier value={endpointId} full copyLabel="Copy Iroh ID" />
          <p data-testid="witness-chosen-by" className="text-sm text-muted-foreground">
            {chosenBy === 0
              ? "None of your identities has chosen it."
              : `Chosen by ${chosenBy} of your identities.`}
            {summary?.is_node_default === true ? " This node uses it by default." : ""}
          </p>
        </div>
        <Section
          testId="witness-ledgers"
          title="What this witness holds"
          description={`${chosen.sentence} Asked when this page loaded, so a record missing here may be on another witness.`}
          action={
            <div className="flex flex-wrap gap-1">
              {FILTERS.map((filter) => (
                <Button
                  key={filter.key}
                  type="button"
                  size="sm"
                  variant={holdings === filter.key ? "default" : "outline"}
                  aria-pressed={holdings === filter.key}
                  data-testid={`witness-holdings-${filter.key}`}
                  onClick={() => setHoldings(filter.key)}
                >
                  {filter.label}
                </Button>
              ))}
            </div>
          }
        >
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
              empty={
                holdings === "all"
                  ? "This witness holds no record."
                  : "No record it holds matches this."
              }
              emptyTestId="witness-ledgers-empty"
            />
          )}
        </Section>
      </PageSections>
    </IdentityPillScope>
  );
}
