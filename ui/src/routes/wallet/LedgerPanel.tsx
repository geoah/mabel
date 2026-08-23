import { useState } from "react";

import { getIdentityLedger } from "@/api/client";
import type { LedgerEvent } from "@/api/types";
import { DeclaredKindNote, DeclaredKindValue } from "@/components/DeclaredKind";
import { DeveloperOnly } from "@/components/DeveloperMode";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useResource } from "@/hooks/useResource";
import { formatTimestamp } from "@/lib/time";

const DEFAULT_LIMIT = 8;

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
 * sequence and its type, each opening into the event detail. `?since=` is
 * inclusive, so a page starts at `seq === since`.
 */
export function LedgerPanel({ identityId, version }: { identityId: string; version: number }) {
  const [since, setSince] = useState(0);
  const [limit, setLimit] = useState(DEFAULT_LIMIT);
  const [sinceInput, setSinceInput] = useState("0");
  const [limitInput, setLimitInput] = useState(String(DEFAULT_LIMIT));
  const [opened, setOpened] = useState<ReadonlySet<number>>(new Set());
  const page = useResource(
    () => getIdentityLedger(identityId, { since, limit }),
    [identityId, since, limit, version],
  );

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
            <Table stack="none" data-testid="ledger-events">
              <TableHeader>
                <TableRow>
                  <TableHead className="w-12">seq</TableHead>
                  <TableHead>event</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {page.data.events.map((event) => {
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
                      <TableRow
                        key={`${event.event_id}-detail`}
                        data-testid={`event-detail-${event.seq}`}
                      >
                        <TableCell colSpan={2} className="bg-muted/40">
                          <EventDetail event={event} />
                        </TableCell>
                      </TableRow>
                    ) : null,
                  ];
                })}
              </TableBody>
            </Table>
            <p className="text-xs text-muted-foreground">
              <span data-testid="ledger-event-count">{page.data.event_count}</span> events in this
              ledger, head at seq{" "}
              <span data-testid="ledger-head-seq">{page.data.head_seq}</span>, showing from seq{" "}
              <span data-testid="ledger-page-since">{page.data.since}</span>, more{" "}
              <span data-testid="ledger-more">{String(page.data.more)}</span>
            </p>
            <div className="flex flex-wrap items-end gap-x-4 gap-y-2">
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  disabled={since === 0}
                  data-testid="ledger-previous"
                  onClick={() => {
                    const next = Math.max(0, since - limit);
                    setSince(next);
                    setSinceInput(String(next));
                  }}
                >
                  Previous
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={!page.data.more}
                  data-testid="ledger-next"
                  onClick={() => {
                    const next = since + limit;
                    setSince(next);
                    setSinceInput(String(next));
                  }}
                >
                  Next
                </Button>
              </div>
              <div className="flex items-end gap-2">
                <div className="space-y-1">
                  <Label htmlFor="ledger-since">since</Label>
                  <Input
                    id="ledger-since"
                    data-testid="ledger-since"
                    value={sinceInput}
                    onChange={(event) => setSinceInput(event.target.value)}
                    className="w-20"
                  />
                </div>
                <div className="space-y-1">
                  <Label htmlFor="ledger-limit">limit</Label>
                  <Input
                    id="ledger-limit"
                    data-testid="ledger-limit"
                    value={limitInput}
                    onChange={(event) => setLimitInput(event.target.value)}
                    className="w-20"
                  />
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  data-testid="ledger-load"
                  onClick={() => {
                    setSince(Number(sinceInput));
                    setLimit(Number(limitInput) || DEFAULT_LIMIT);
                  }}
                >
                  Load
                </Button>
              </div>
            </div>
            <DeveloperOnly>
              <KeyValueTable>
                <KeyValue label="declared kind" testId="ledger-declared-kind-row">
                  <DeclaredKindValue
                    kind={page.data.declared_kind}
                    testId="ledger-declared-kind"
                  />
                </KeyValue>
                <KeyValue label="ledger_id" testId="ledger-id">
                  <Identifier value={page.data.ledger_id} />
                </KeyValue>
                <KeyValue label="head_event" testId="ledger-head-event">
                  <Identifier value={page.data.head_event} />
                </KeyValue>
              </KeyValueTable>
              <DeclaredKindNote testId="ledger-declared-kind-note" />
            </DeveloperOnly>
          </>
        )}
      </CardContent>
    </Card>
  );
}
