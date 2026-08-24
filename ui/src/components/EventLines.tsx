import { useState } from "react";

import type { LedgerEvent } from "@/api/types";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { formatTimestamp } from "@/lib/time";

/**
 * What each entry did to the record, in one clause a reader can act on. A kind
 * this build does not know renders without a gloss rather than guessing at one.
 */
const GLOSS: Record<string, string> = {
  inception: "created this identity",
  profile_update: "changed the public name, email and website",
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
 * The ledger as compact rows, not a table: two columns on one tight line each,
 * the position and what the entry did, and each row opens into the entry itself
 * (proposal 005). The wallet's own ledger and a witness's copy of it render
 * through this one component, because it is the same ledger.
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
    <ul data-testid="ledger-events" className="divide-y">
      {events.map((event) => {
        const open = opened.has(event.seq);
        return (
          <li key={event.event_id} data-testid={`ledger-event-${event.seq}`}>
            <button
              type="button"
              data-testid={`event-expand-${event.seq}`}
              aria-expanded={open}
              onClick={() => toggle(event.seq)}
              className="flex w-full items-baseline gap-2 py-1 text-left focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
            >
              <span aria-hidden="true" className="w-3 shrink-0 text-muted-foreground">
                {open ? "−" : "+"}
              </span>
              <span
                data-testid={`event-seq-${event.seq}`}
                className="w-6 shrink-0 font-mono text-xs text-muted-foreground"
              >
                {event.seq}
              </span>
              <span className="min-w-0 flex-1">
                <span data-testid={`event-gloss-${event.seq}`} className="text-sm">
                  {GLOSS[event.payload_kind] ?? ""}
                </span>{" "}
                <span
                  data-testid={`event-payload-kind-${event.seq}`}
                  className="font-mono text-xs text-muted-foreground"
                >
                  {event.payload_kind}
                </span>
              </span>
            </button>
            {open && (
              <div
                data-testid={`event-detail-${event.seq}`}
                className="rounded-md bg-muted/40 px-2 py-1"
              >
                <EventDetail event={event} />
              </div>
            )}
          </li>
        );
      })}
    </ul>
  );
}
