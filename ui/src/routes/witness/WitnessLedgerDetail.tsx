import { Link, useParams } from "react-router";

import { getLedger, getLedgerEvents } from "@/api/client";
import { DeclaredKindNote, DeclaredKindValue } from "@/components/DeclaredKind";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { EventLines } from "@/components/EventLines";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

import { ForksPanel } from "./ForksPanel";
import { WITNESS_HOLDINGS_NOTE, WITNESS_READ_ONLY_NOTE } from "./notes";

/** The chain this witness stored for one ledger, one line per event. */
function WitnessLedgerEvents({ ledgerId }: { ledgerId: string }) {
  const page = useResource(() => getLedgerEvents(ledgerId, { limit: 512 }), [ledgerId]);

  return (
    <Card data-testid="ledger-panel">
      <CardHeader>
        <CardTitle>Ledger</CardTitle>
        <CardDescription>
          One line per event: open a line for the event it records
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {page.loading && <p data-testid="ledger-loading">loading</p>}
        {page.error && <ErrorEnvelopeView error={page.error} testId="ledger-error" />}
        {page.data && (
          <>
            <EventLines events={page.data.events} />
            <p className="text-xs text-muted-foreground">
              <span data-testid="ledger-event-count">{page.data.event_count}</span> events in this
              ledger, head at seq{" "}
              <span data-testid="ledger-head-seq">{page.data.head_seq}</span>
            </p>
          </>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * One ledger as this witness holds it, drawn as the identity page: the overview,
 * the chain and the fork records when there are any (proposal 004). Every
 * request the route issues is a read.
 */
export function WitnessLedgerDetail() {
  const { ledgerId = "" } = useParams();
  const ledger = useResource(() => getLedger(ledgerId), [ledgerId]);

  return (
    <div className="space-y-4">
      <Link
        to="/witness"
        className="inline-flex min-h-10 items-center text-sm underline"
        data-testid="witness-ledger-back"
      >
        Ledgers
      </Link>
      {ledger.loading && <p data-testid="witness-ledger-detail-loading">loading</p>}
      {ledger.error && (
        <ErrorEnvelopeView error={ledger.error} testId="witness-ledger-detail-error" />
      )}
      {ledger.data && (
        <>
          <Card data-testid="witness-ledger-detail">
            <CardHeader>
              <CardTitle>
                <Identifier value={ledger.data.entry.ledger_id} />
              </CardTitle>
              <CardDescription data-testid="witness-read-only-note">
                {WITNESS_READ_ONLY_NOTE}
              </CardDescription>
            </CardHeader>
            <CardContent className="space-y-2">
              <KeyValueTable>
                <KeyValue label="ledger_id" testId="witness-detail-ledger-id">
                  <Identifier value={ledger.data.entry.ledger_id} />
                </KeyValue>
                <KeyValue label="declared kind" testId="witness-detail-declared-kind-row">
                  <DeclaredKindValue
                    kind={ledger.data.entry.declared_kind}
                    testId="witness-detail-declared-kind"
                  />
                </KeyValue>
                <KeyValue label="ledger" testId="witness-detail-ledger-summary">
                  <span data-testid="witness-detail-event-count">
                    {ledger.data.entry.event_count}
                  </span>{" "}
                  events, head at seq{" "}
                  <span data-testid="witness-detail-head-seq">{ledger.data.entry.head_seq}</span>
                </KeyValue>
                <KeyValue label="head_event" testId="witness-detail-head-event">
                  <Identifier value={ledger.data.entry.head_event} />
                </KeyValue>
                <KeyValue label="fork records" testId="witness-detail-fork-count">
                  {ledger.data.entry.fork_count}
                  {ledger.data.entry.forks_truncated
                    ? ", and this witness stopped recording them, so that count is a floor"
                    : ""}
                </KeyValue>
                <KeyValue label="first seen" testId="witness-detail-first-seen-ms">
                  {ledger.data.entry.first_seen_ms}
                </KeyValue>
                <KeyValue label="updated" testId="witness-detail-updated-ms">
                  {ledger.data.entry.updated_ms}
                </KeyValue>
                <KeyValue label="source_endpoint" testId="witness-detail-source-endpoint">
                  <Identifier value={ledger.data.entry.source_endpoint} />
                </KeyValue>
                <KeyValue label="witnesses" testId="witness-detail-witnesses">
                  {ledger.data.witnesses.length === 0 ? (
                    "none"
                  ) : (
                    <span className="flex flex-col gap-1">
                      {ledger.data.witnesses.map((witness) => (
                        <Identifier key={witness} value={witness} />
                      ))}
                    </span>
                  )}
                </KeyValue>
              </KeyValueTable>
              <DeclaredKindNote testId="witness-detail-declared-kind-note" />
              <p
                className="text-xs text-muted-foreground"
                data-testid="witness-detail-holdings-note"
              >
                {WITNESS_HOLDINGS_NOTE}
              </p>
            </CardContent>
          </Card>
          <WitnessLedgerEvents ledgerId={ledger.data.entry.ledger_id} />
          <ForksPanel ledgerId={ledger.data.entry.ledger_id} />
        </>
      )}
    </div>
  );
}
