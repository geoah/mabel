import type { LedgerEvent } from "@/api/types";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import {
  Collapsible,
  CollapsibleChevron,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { mabelId } from "@/lib/link";
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
  endpoint_advertisement: "published the endpoints that answer for it",
  trust_attestation: "said it trusts someone",
  trust_revocation: "took back trusting someone",
  membership_invitation: "invited someone to help control this identity",
  membership_acceptance: "confirmed someone as a controller",
  membership_removal: "removed someone",
};

/** What a kind this build does not know is called on the closed line. */
const UNKNOWN_GLOSS = "did something this version does not know about";

/**
 * Where an identity id sits inside each kind of payload, so the contents a
 * reader opens name identities the way the rest of the screen does
 * (decision 019).
 *
 * Kind by kind, never by field name, because one name means two things: `target`
 * is an identity under `membership_removal` and the entry id of an attestation
 * under `trust_revocation`, and `witnesses` holds identity ids under
 * `witness_set` and endpoints under the retired `witness_config`. Keys, entry ids,
 * signatures and the endpoints of `endpoint_advertisement` are named by nothing
 * here and stay bare.
 */
const IDENTITY_PATHS: Record<string, string[][]> = {
  inception: [["root", "identity_root", "founder"]],
  trust_attestation: [["subject"]],
  membership_invitation: [["invitee"]],
  membership_removal: [["target"]],
  witness_set: [["witnesses"]],
};

/** One value, or every value of one list, as a person reads an identity id. */
function shownValue(value: unknown): unknown {
  if (typeof value === "string") {
    return mabelId(value);
  }
  if (Array.isArray(value)) {
    return value.map(shownValue);
  }
  return value;
}

/** A copy of `payload` with the identity ids at `path` prefixed. */
function withShownPath(payload: unknown, path: string[]): unknown {
  if (payload === null || typeof payload !== "object" || Array.isArray(payload)) {
    return payload;
  }
  const [head, ...rest] = path;
  const fields = payload as Record<string, unknown>;
  if (!(head in fields)) {
    return payload;
  }
  return {
    ...fields,
    [head]: rest.length === 0 ? shownValue(fields[head]) : withShownPath(fields[head], rest),
  };
}

/**
 * The payload as it is shown: the document the node sent, with every identity
 * id in it carrying the prefix. The document itself is untouched, and what a
 * `--json` run or the API returns is the bare id.
 */
export function shownPayload(kind: string, payload: Record<string, unknown>): string {
  let shown: unknown = payload;
  for (const path of IDENTITY_PATHS[kind] ?? []) {
    shown = withShownPath(shown, path);
  }
  return JSON.stringify(shown);
}

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
        <span className="font-mono text-xs break-all">
          {shownPayload(event.payload_kind, event.payload)}
        </span>
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
