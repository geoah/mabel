import { useParams } from "react-router";

import { getLedger, getLedgerEvents } from "@/api/client";
import type { LedgerSummary } from "@/api/types";
import { DeclaredKindValue } from "@/components/DeclaredKind";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { EventLines } from "@/components/EventLines";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { PageSections, Section } from "@/components/Section";
import { usePagedList } from "@/hooks/usePagedList";
import { useResource } from "@/hooks/useResource";
import { formatTimestamp } from "@/lib/time";

import { ForksPanel } from "./ForksPanel";
import { WITNESS_HOLDINGS_NOTE, WITNESS_READ_ONLY_NOTE } from "./notes";

/** How many entries this screen reads before it says it stopped. */
const EVENT_CAP = 4096;

/** How many entries one request asks for. */
const EVENT_PAGE = 512;

/**
 * The record this witness stored for one identity, one line per entry. This
 * screen draws the whole chain rather than pages of it, so it follows the
 * route's `more` to the end, up to a cap.
 */
function WitnessLedgerEvents({ entry }: { entry: LedgerSummary }) {
  const page = usePagedList(
    // A stored chain runs from seq 0 without gaps and `?since=` is inclusive, so
    // the number of entries already read is the next `since`.
    (since, limit) =>
      getLedgerEvents(entry.ledger_id, { since, limit }).then((response) => ({
        items: response.events,
        more: response.more,
      })),
    [entry.ledger_id],
    { pageSize: EVENT_PAGE, cap: EVENT_CAP },
  );

  return (
    <Section
      testId="ledger-panel"
      title="Ledger"
      description="Everything this identity has signed, oldest first. Open a line to read the entry."
    >
      {page.loading && <p data-testid="ledger-loading">loading</p>}
      {page.error && <ErrorEnvelopeView error={page.error} testId="ledger-error" />}
      {page.capped && (
        <p data-testid="ledger-capped" className="text-sm">
          Showing the first {page.items.length} entries. This record has more.
        </p>
      )}
      {page.loaded && (
        <>
          <EventLines events={page.items} />
          <p className="text-xs text-muted-foreground">
            <span data-testid="ledger-event-count">{entry.event_count}</span>{" "}
            {entry.event_count === 1 ? "entry" : "entries"} on this record, the newest at position{" "}
            <span data-testid="ledger-head-seq">{entry.head_seq}</span>.
          </p>
        </>
      )}
    </Section>
  );
}

/**
 * One record as this witness holds it, drawn as the identity page: the overview,
 * the entries and the conflicts when there are any (proposal 004). Every request
 * the route issues is a read.
 */
export function WitnessLedgerDetail() {
  const { ledgerId = "" } = useParams();
  const ledger = useResource(() => getLedger(ledgerId), [ledgerId]);

  return (
    <PageSections>
      {ledger.loading && <p data-testid="witness-ledger-detail-loading">loading</p>}
      {ledger.error && (
        <ErrorEnvelopeView error={ledger.error} testId="witness-ledger-detail-error" />
      )}
      {ledger.data && (
        <>
          {/* The way back to the records is the nav, so this page draws no back
              link of its own. */}
          <section data-testid="witness-ledger-detail" className="space-y-3">
            <div className="space-y-2">
              <h1 className="text-2xl leading-tight font-semibold tracking-tight">This record</h1>
              <Identifier value={ledger.data.entry.ledger_id} full copyLabel="Copy Mabel ID" />
              <p data-testid="witness-read-only-note" className="text-sm text-muted-foreground">
                {WITNESS_READ_ONLY_NOTE}
              </p>
            </div>
            <KeyValueTable>
              <KeyValue label="Mabel ID" testId="witness-detail-ledger-id">
                <Identifier value={ledger.data.entry.ledger_id} copyLabel="Copy Mabel ID" />
              </KeyValue>
              <KeyValue label="declared kind" testId="witness-detail-declared-kind-row">
                <DeclaredKindValue
                  kind={ledger.data.entry.declared_kind}
                  testId="witness-detail-declared-kind"
                />
              </KeyValue>
              <KeyValue label="record" testId="witness-detail-ledger-summary">
                <span data-testid="witness-detail-event-count">
                  {ledger.data.entry.event_count}
                </span>{" "}
                {ledger.data.entry.event_count === 1 ? "entry" : "entries"}, the newest at position{" "}
                <span data-testid="witness-detail-head-seq">{ledger.data.entry.head_seq}</span>
              </KeyValue>
              <KeyValue label="newest entry" testId="witness-detail-head-event">
                <Identifier value={ledger.data.entry.head_event} />
              </KeyValue>
              <KeyValue label="conflicts" testId="witness-detail-fork-count">
                {ledger.data.entry.fork_count}
                {ledger.data.entry.forks_truncated
                  ? ", and this witness stopped recording more, so there may be others"
                  : ""}
              </KeyValue>
              <KeyValue label="first seen" testId="witness-detail-first-seen-ms">
                {formatTimestamp(ledger.data.entry.first_seen_ms)}
              </KeyValue>
              <KeyValue label="last updated" testId="witness-detail-updated-ms">
                {formatTimestamp(ledger.data.entry.updated_ms)}
              </KeyValue>
              <KeyValue label="learned from" testId="witness-detail-source-endpoint">
                <Identifier value={ledger.data.entry.source_endpoint} copyLabel="Copy Iroh ID" />
              </KeyValue>
              <KeyValue label="who keeps a copy" testId="witness-detail-witnesses">
                {ledger.data.witnesses.length === 0 ? (
                  "none"
                ) : (
                  <span className="flex flex-col gap-1">
                    {ledger.data.witnesses.map((witness) => (
                      <Identifier key={witness} value={witness} copyLabel="Copy Iroh ID" />
                    ))}
                  </span>
                )}
              </KeyValue>
            </KeyValueTable>
            <p className="text-xs text-muted-foreground" data-testid="witness-detail-holdings-note">
              {WITNESS_HOLDINGS_NOTE}
            </p>
          </section>
          <WitnessLedgerEvents entry={ledger.data.entry} />
          <ForksPanel ledgerId={ledger.data.entry.ledger_id} />
        </>
      )}
    </PageSections>
  );
}
