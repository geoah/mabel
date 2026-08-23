import { listForks } from "@/api/client";
import type { ForkRecord, LedgerEvent } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Field, FieldGrid } from "@/components/Field";
import { Identifier } from "@/components/Identifier";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

import { FORK_EVIDENCE_NOTE } from "./notes";

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
      <FieldGrid className="sm:grid-cols-[7rem_minmax(0,1fr)]">
        <Field label="event_id" testId={`${testId}-event-id`}>
          <Identifier value={event.event_id} />
        </Field>
        <Field label="seq" testId={`${testId}-seq`}>
          {event.seq}
        </Field>
        <Field label="prev" testId={`${testId}-prev`}>
          <Identifier value={event.prev} />
        </Field>
        <Field label="timestamp_ms" testId={`${testId}-timestamp-ms`}>
          {event.timestamp_ms}
        </Field>
        <Field label="author_key" testId={`${testId}-author-key`}>
          <Identifier value={event.author_key} />
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
        <Field label="seq" testId={`fork-seq-${key}`}>
          {record.seq}
        </Field>
        <Field label="observed_ms" testId={`fork-observed-ms-${key}`}>
          {record.observed_ms}
        </Field>
        <Field label="source_endpoint" testId={`fork-source-endpoint-${key}`}>
          <Identifier value={record.source_endpoint} />
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
 * The fork records one ledger carries, drawn on its identity page and nowhere
 * else (proposal 004). Both events of a record are shown, so a reader checks
 * the conflict without a second request (proposal 001 section 5). A ledger with
 * no records renders nothing at all.
 */
export function ForksPanel({ ledgerId }: { ledgerId: string }) {
  const page = useResource(() => listForks({ ledger_id: ledgerId, limit: 64 }), [ledgerId]);

  if (page.error === null && (page.data === null || page.data.entries.length === 0)) {
    return null;
  }

  return (
    <Card data-testid="witness-forks">
      <CardHeader>
        <CardTitle>Forks</CardTitle>
        <CardDescription data-testid="fork-evidence-note">{FORK_EVIDENCE_NOTE}</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {page.error && <ErrorEnvelopeView error={page.error} testId="witness-forks-error" />}
        {page.data?.entries.map((record) => (
          <ForkRecordView key={`${record.ledger_id}-${record.seq}`} record={record} />
        ))}
      </CardContent>
    </Card>
  );
}
