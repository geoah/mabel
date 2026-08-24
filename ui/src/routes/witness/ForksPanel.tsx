import { listForks } from "@/api/client";
import type { ForkRecord, LedgerEvent } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Field, FieldGrid } from "@/components/Field";
import { Identifier } from "@/components/Identifier";
import { Section } from "@/components/Section";
import { usePagedList } from "@/hooks/usePagedList";
import { formatTimestamp } from "@/lib/time";

import { FORK_EVIDENCE_NOTE } from "./notes";

/** How many conflict records this panel draws before it says it stopped. */
const FORK_CAP = 256;

/** One of the two entries at the conflicting position, with every field it carries. */
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
    // No border: this pane sits inside the bordered conflict record, and the
    // sentence above it is what tells the two entries apart.
    <div data-testid={testId}>
      <p className="mb-2 text-xs font-medium">
        {side === "kept"
          ? "kept: the entry this witness stored first"
          : "conflicting: recorded as evidence, not stored"}
      </p>
      <FieldGrid className="sm:grid-cols-[7rem_minmax(0,1fr)]">
        <Field label="entry id" testId={`${testId}-event-id`}>
          <Identifier value={event.event_id} />
        </Field>
        <Field label="position" testId={`${testId}-seq`}>
          {event.seq}
        </Field>
        <Field label="the entry before it" testId={`${testId}-prev`}>
          <Identifier value={event.prev} />
        </Field>
        <Field label="signed at" testId={`${testId}-timestamp-ms`}>
          {formatTimestamp(event.timestamp_ms)}
        </Field>
        <Field label="signed with" testId={`${testId}-author-key`}>
          <Identifier value={event.author_key} />
        </Field>
        <Field label="kind" testId={`${testId}-payload-kind`}>
          {event.payload_kind}
        </Field>
        <Field label="what it says" testId={`${testId}-payload`} mono>
          {JSON.stringify(event.payload)}
        </Field>
      </FieldGrid>
    </div>
  );
}

/** One conflict: where it came from, what the node says about it, and both entries. */
function ForkRecordView({ record }: { record: ForkRecord }) {
  const key = `${record.ledger_id}-${record.seq}`;
  return (
    <div className="space-y-3 rounded-md border p-3" data-testid={`fork-record-${key}`}>
      <FieldGrid>
        <Field label="position" testId={`fork-seq-${key}`}>
          {record.seq}
        </Field>
        <Field label="noticed at" testId={`fork-observed-ms-${key}`}>
          {formatTimestamp(record.observed_ms)}
        </Field>
        <Field label="learned from" testId={`fork-source-endpoint-${key}`}>
          <Identifier value={record.source_endpoint} />
        </Field>
      </FieldGrid>
      <p className="text-xs" data-testid={`fork-statement-${key}`}>
        {record.statement}
      </p>
      <div className="grid gap-3 divide-y">
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
 * The conflicts one record carries, drawn on its identity page and nowhere else
 * (proposal 004). Both entries are shown, so a reader checks the conflict
 * without a second request (proposal 001 section 5). A record with no conflicts
 * renders nothing at all.
 */
export function ForksPanel({ ledgerId }: { ledgerId: string }) {
  // /api/forks pages, and a record with more conflicts than one page is exactly
  // the record whose conflicts a reader came for: the panel reads them all, to a
  // cap, and says so when the cap cut the list short.
  const page = usePagedList(
    (offset, limit) =>
      listForks({ ledger_id: ledgerId, offset, limit }).then((response) => ({
        items: response.entries,
        more: response.more,
      })),
    [ledgerId],
    { pageSize: 64, cap: FORK_CAP },
  );

  if (page.error === null && page.items.length === 0) {
    return null;
  }

  return (
    <Section
      testId="witness-forks"
      title="Conflicts"
      descriptionTestId="fork-evidence-note"
      description={FORK_EVIDENCE_NOTE}
    >
      {page.error && <ErrorEnvelopeView error={page.error} testId="witness-forks-error" />}
      {page.capped && (
        <p data-testid="witness-forks-capped" className="text-sm">
          Showing the first {page.items.length} conflicts. This record has more.
        </p>
      )}
      {page.items.map((record) => (
        <ForkRecordView key={`${record.ledger_id}-${record.seq}`} record={record} />
      ))}
    </Section>
  );
}
