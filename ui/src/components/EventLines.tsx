import { useState } from "react";

import type { LedgerEvent } from "@/api/types";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { formatTimestamp } from "@/lib/time";

/**
 * What each entry did to the record, in one clause a reader can act on. A kind
 * this build does not know renders without a gloss rather than guessing at one.
 */
const GLOSS: Record<string, string> = {
  inception: "created this identity",
  profile_update: "changed the public name and website",
  witness_config: "chose who keeps a copy",
  trust_attestation: "said it trusts someone",
  trust_revocation: "took back trusting someone",
  membership_invitation: "invited someone to help control this identity",
  membership_acceptance: "confirmed someone as a controller",
  membership_removal: "removed someone",
};

/** One entry, opened: the fields the record carries, contents last. */
function EventDetail({ event }: { event: LedgerEvent }) {
  return (
    <KeyValueTable>
      <KeyValue label="entry id" testId={`event-id-${event.seq}`}>
        <Identifier value={event.event_id} />
      </KeyValue>
      <KeyValue label="the entry before it" testId={`event-prev-${event.seq}`}>
        <Identifier value={event.prev} />
      </KeyValue>
      <KeyValue label="signed at" testId={`event-timestamp-${event.seq}`}>
        {formatTimestamp(event.timestamp_ms)}
      </KeyValue>
      <KeyValue label="what it says" testId={`event-payload-${event.seq}`}>
        <span className="font-mono text-xs break-all">{JSON.stringify(event.payload)}</span>
      </KeyValue>
    </KeyValueTable>
  );
}

/**
 * The record as decision 014 asks for it: one line per entry carrying its
 * position and what it did, each opening into the entry itself. The wallet's own
 * record and a witness's copy of it render through this one component, because
 * the record is the same record.
 */
export function EventLines({ events }: { events: LedgerEvent[] }) {
  const [opened, setOpened] = useState<ReadonlySet<number>>(new Set());

  function toggle(seq: number) {
    setOpened((current) => {
      const next = new Set(current);
      if (!next.delete(seq)) {
        next.add(seq);
      }
      return next;
    });
  }

  return (
    <Table stack="none" data-testid="ledger-events">
      <TableHeader>
        <TableRow>
          <TableHead className="w-12">at</TableHead>
          <TableHead>what happened</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {events.map((event) => {
          const open = opened.has(event.seq);
          return [
            <TableRow key={event.event_id} data-testid={`ledger-event-${event.seq}`}>
              <TableCell
                label="at"
                data-testid={`event-seq-${event.seq}`}
                className="align-top font-mono text-xs"
              >
                {event.seq}
              </TableCell>
              <TableCell className="align-top">
                <button
                  type="button"
                  data-testid={`event-expand-${event.seq}`}
                  aria-expanded={open}
                  onClick={() => toggle(event.seq)}
                  className="flex min-h-9 w-full flex-wrap items-baseline gap-x-2 text-left focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
                >
                  <span aria-hidden="true" className="text-muted-foreground">
                    {open ? "−" : "+"}
                  </span>
                  <span
                    data-testid={`event-payload-kind-${event.seq}`}
                    className="font-mono text-xs"
                  >
                    {event.payload_kind}
                  </span>
                  <span
                    data-testid={`event-gloss-${event.seq}`}
                    className="text-xs text-muted-foreground"
                  >
                    {GLOSS[event.payload_kind] ?? ""}
                  </span>
                </button>
              </TableCell>
            </TableRow>,
            open ? (
              <TableRow key={`${event.event_id}-detail`} data-testid={`event-detail-${event.seq}`}>
                <TableCell colSpan={2} className="bg-muted/40">
                  <EventDetail event={event} />
                </TableCell>
              </TableRow>
            ) : null,
          ];
        })}
      </TableBody>
    </Table>
  );
}
