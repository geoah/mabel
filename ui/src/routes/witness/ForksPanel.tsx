import { useState } from "react";
import { Link } from "react-router";

import { listForks } from "@/api/client";
import type { ForkRecord, LedgerEvent } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Field, FieldGrid, Nullable } from "@/components/Field";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

import { FORK_EVIDENCE_NOTE, WITNESS_HOLDINGS_NOTE } from "./notes";

export const FORK_PAGE_SIZE = 4;

/** One of the two events at the forked sequence, with every field it carries. */
function ForkEventPane({
  event,
  side,
  testId,
}: {
  event: LedgerEvent;
  side: "kept" | "conflicting";
  testId: string;
}) {
  return (
    <div className="rounded-md border p-3" data-testid={testId}>
      <p className="mb-2 text-xs font-medium">
        {side === "kept" ? "kept, the event stored first" : "conflicting, recorded not stored"}
      </p>
      <FieldGrid className="grid-cols-[7rem_1fr]">
        <Field label="event_id" testId={`${testId}-event-id`} mono>
          {event.event_id}
        </Field>
        <Field label="seq" testId={`${testId}-seq`}>
          {event.seq}
        </Field>
        <Field label="prev" testId={`${testId}-prev`} mono>
          <Nullable value={event.prev} />
        </Field>
        <Field label="timestamp_ms" testId={`${testId}-timestamp-ms`}>
          {event.timestamp_ms}
        </Field>
        <Field label="author_key" testId={`${testId}-author-key`} mono>
          {event.author_key}
        </Field>
        <Field label="payload_kind" testId={`${testId}-payload-kind`}>
          {event.payload_kind}
        </Field>
        <Field label="payload" testId={`${testId}-payload`} mono>
          {JSON.stringify(event.payload)}
        </Field>
      </FieldGrid>
    </div>
  );
}

/** One ForkRecord: its provenance, its statement and both events side by side. */
function ForkRecordView({ record }: { record: ForkRecord }) {
  const key = `${record.ledger_id}-${record.seq}`;
  return (
    <div className="space-y-3 rounded-md border p-3" data-testid={`fork-record-${key}`}>
      <FieldGrid>
        <Field label="ledger_id" testId={`fork-ledger-id-${key}`} mono>
          <Link
            to={`/witness/ledgers/${record.ledger_id}`}
            className="underline"
            data-testid={`fork-ledger-link-${key}`}
          >
            {record.ledger_id}
          </Link>
        </Field>
        <Field label="seq" testId={`fork-seq-${key}`}>
          {record.seq}
        </Field>
        <Field label="observed_ms" testId={`fork-observed-ms-${key}`}>
          {record.observed_ms}
        </Field>
        <Field label="source_endpoint" testId={`fork-source-endpoint-${key}`} mono>
          {record.source_endpoint}
        </Field>
      </FieldGrid>
      <p className="text-xs" data-testid={`fork-statement-${key}`}>
        {record.statement}
      </p>
      <div className="grid gap-3 lg:grid-cols-2">
        <ForkEventPane event={record.kept} side="kept" testId={`fork-kept-${key}`} />
        <ForkEventPane
          event={record.conflicting}
          side="conflicting"
          testId={`fork-conflicting-${key}`}
        />
      </div>
    </div>
  );
}

/**
 * GET /api/forks, paged by offset and optionally filtered to one ledger. Both
 * events of a record are shown, so a reader checks the conflict without a
 * second request (proposal 001 section 5).
 */
export function ForksPanel({ ledgerId }: { ledgerId?: string }) {
  const [offset, setOffset] = useState(0);
  const page = useResource(
    () => listForks({ ledger_id: ledgerId, offset, limit: FORK_PAGE_SIZE }),
    [ledgerId, offset],
  );

  return (
    <Card data-testid="witness-forks">
      <CardHeader>
        <CardTitle>Forks</CardTitle>
        <CardDescription data-testid="fork-evidence-note">{FORK_EVIDENCE_NOTE}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {ledgerId && (
          <p className="text-xs text-muted-foreground" data-testid="witness-forks-filter">
            filtered to ledger_id {ledgerId}
          </p>
        )}
        {page.loading && <p data-testid="witness-forks-loading">loading</p>}
        {page.error && <ErrorEnvelopeView error={page.error} testId="witness-forks-error" />}
        {page.data && page.data.entries.length === 0 && (
          <p data-testid="witness-forks-empty">
            this witness recorded no fork{ledgerId ? " for this ledger" : ""}
          </p>
        )}
        {page.data?.entries.map((record) => (
          <ForkRecordView key={`${record.ledger_id}-${record.seq}`} record={record} />
        ))}
        {page.data && (
          <>
            <p className="text-xs text-muted-foreground" data-testid="witness-forks-holdings-note">
              {WITNESS_HOLDINGS_NOTE}
            </p>
            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={offset === 0}
                data-testid="witness-forks-previous"
                onClick={() => setOffset(Math.max(0, offset - FORK_PAGE_SIZE))}
              >
                Previous
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={!page.data.more}
                data-testid="witness-forks-next"
                onClick={() => setOffset(offset + FORK_PAGE_SIZE)}
              >
                Next
              </Button>
              <span className="text-xs text-muted-foreground">
                <span data-testid="witness-forks-offset">offset {page.data.offset}</span>
                {", "}
                <span data-testid="witness-forks-more">more {String(page.data.more)}</span>
              </span>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
