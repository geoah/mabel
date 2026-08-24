import type { LedgerEvent } from "@/api/types";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import {
  Collapsible,
  CollapsibleChevron,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { formatTimestamp } from "@/lib/time";

/**
 * What each entry did to the record, in one clause a reader can act on. A kind
 * this build does not know renders without a gloss rather than guessing at one.
 */
const GLOSS: Record<string, string> = {
  inception: "created this identity",
  profile_update: "changed the public name, email and handle",
  witness_config: "chose who keeps a copy",
  witness_set: "chose who keeps a copy",
  endpoint_advertisement: "published the machines that answer for it",
  trust_attestation: "said it trusts someone",
  trust_revocation: "took back trusting someone",
  membership_invitation: "invited someone to help control this identity",
  membership_acceptance: "confirmed someone as a controller",
  membership_removal: "removed someone",
};

/** What a kind this build does not know is called on the closed line. */
const UNKNOWN_GLOSS = "did something this version does not know about";

/**
 * One entry, opened: the fields the record carries, contents last. The raw kind
 * string and the payload live here and nowhere else, because neither is a thing
 * a reader of a ledger line can act on.
 */
function EventDetail({ event }: { event: LedgerEvent }) {
  return (
    <KeyValueTable>
      <KeyValue label="kind" testId={`event-payload-kind-${event.seq}`}>
        <span className="font-mono text-xs">{event.payload_kind}</span>
      </KeyValue>
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
  return (
    <ul data-testid="ledger-events" className="divide-y">
      {events.map((event) => (
        <li key={event.event_id} data-testid={`ledger-event-${event.seq}`}>
          <Collapsible>
            <CollapsibleTrigger
              data-testid={`event-expand-${event.seq}`}
              className="flex w-full items-baseline gap-2 rounded-md px-1 py-2 text-left hover:bg-accent"
            >
              <CollapsibleChevron className="translate-y-0.5" />
              <span
                data-testid={`event-seq-${event.seq}`}
                className="w-6 shrink-0 font-mono text-xs text-muted-foreground"
              >
                {event.seq}
              </span>
              <span data-testid={`event-gloss-${event.seq}`} className="min-w-0 flex-1 text-sm">
                {GLOSS[event.payload_kind] ?? UNKNOWN_GLOSS}
              </span>
            </CollapsibleTrigger>
            <CollapsibleContent
              data-testid={`event-detail-${event.seq}`}
              className="rounded-md bg-muted/40 px-2 py-1 pl-8"
            >
              <EventDetail event={event} />
            </CollapsibleContent>
          </Collapsible>
        </li>
      ))}
    </ul>
  );
}
