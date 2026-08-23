import { useState } from "react";

import type { LedgerEvent } from "@/api/types";
import { DeveloperOnly } from "@/components/DeveloperMode";
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
 * What each payload kind did to the ledger, in one clause. A kind this build
 * does not know renders without a gloss rather than guessing at one.
 */
const GLOSS: Record<string, string> = {
  inception: "created this ledger",
  profile_update: "replaced the display name and hostname",
  witness_config: "replaced the witness set",
  trust_attestation: "attested trust in an identity",
  trust_revocation: "revoked an earlier attestation",
  membership_invitation: "invited an identity",
  membership_acceptance: "admitted an invited identity",
  membership_removal: "removed a principal",
};

/** One event, opened: the fields the chain carries, payload last. */
function EventDetail({ event }: { event: LedgerEvent }) {
  return (
    <KeyValueTable>
      <KeyValue label="event_id" testId={`event-id-${event.seq}`}>
        <Identifier value={event.event_id} />
      </KeyValue>
      <KeyValue label="prev" testId={`event-prev-${event.seq}`}>
        <Identifier value={event.prev} />
      </KeyValue>
      <KeyValue label="timestamp" testId={`event-timestamp-${event.seq}`}>
        {formatTimestamp(event.timestamp_ms)}
      </KeyValue>
      <DeveloperOnly>
        <KeyValue label="timestamp_ms" testId={`event-timestamp-ms-${event.seq}`}>
          {event.timestamp_ms}
        </KeyValue>
        <KeyValue label="author_key" testId={`event-author-key-${event.seq}`}>
          <Identifier value={event.author_key} />
        </KeyValue>
        <KeyValue label="ledger_id" testId={`event-ledger-id-${event.seq}`}>
          <Identifier value={event.ledger_id} />
        </KeyValue>
      </DeveloperOnly>
      <KeyValue label="payload" testId={`event-payload-${event.seq}`}>
        <span className="font-mono text-xs break-all">{JSON.stringify(event.payload)}</span>
      </KeyValue>
    </KeyValueTable>
  );
}

/**
 * The ledger as decision 014 asks for it: one line per event carrying its
 * sequence and its type, each opening into the event detail. The wallet's own
 * ledger and a witness's copy of it render through this one component, because
 * the chain is the same chain.
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
          <TableHead className="w-12">seq</TableHead>
          <TableHead>event</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {events.map((event) => {
          const open = opened.has(event.seq);
          return [
            <TableRow key={event.event_id} data-testid={`ledger-event-${event.seq}`}>
              <TableCell
                label="seq"
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
