import { useState } from "react";

import { getLedgerEvents } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Field, FieldGrid } from "@/components/Field";
import { Identifier } from "@/components/Identifier";
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

const DEFAULT_LIMIT = 8;

/** GET /api/ledgers/:id/events. ?since= is inclusive: the page opens at seq === since. */
export function LedgerEventsPanel({ ledgerId }: { ledgerId: string }) {
  const [since, setSince] = useState(0);
  const [limit, setLimit] = useState(DEFAULT_LIMIT);
  const [sinceInput, setSinceInput] = useState("0");
  const [limitInput, setLimitInput] = useState(String(DEFAULT_LIMIT));
  const page = useResource(
    () => getLedgerEvents(ledgerId, { since, limit }),
    [ledgerId, since, limit],
  );

  return (
    <Card data-testid="witness-events-panel">
      <CardHeader>
        <CardTitle>Events</CardTitle>
        <CardDescription>since is inclusive: the page starts at seq equal to since</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex flex-wrap items-end gap-2">
          <div className="space-y-1">
            <Label htmlFor="witness-events-since">since</Label>
            <Input
              id="witness-events-since"
              data-testid="witness-events-since"
              value={sinceInput}
              onChange={(event) => setSinceInput(event.target.value)}
              className="w-24"
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="witness-events-limit">limit</Label>
            <Input
              id="witness-events-limit"
              data-testid="witness-events-limit"
              value={limitInput}
              onChange={(event) => setLimitInput(event.target.value)}
              className="w-24"
            />
          </div>
          <Button
            variant="outline"
            data-testid="witness-events-load"
            onClick={() => {
              setSince(Number(sinceInput));
              setLimit(Number(limitInput) || DEFAULT_LIMIT);
            }}
          >
            Load
          </Button>
        </div>
        {page.loading && <p data-testid="witness-events-loading">loading</p>}
        {page.error && <ErrorEnvelopeView error={page.error} testId="witness-events-error" />}
        {page.data && (
          <>
            <FieldGrid>
              <Field label="since" testId="witness-events-page-since">
                {page.data.since}
              </Field>
              <Field label="limit" testId="witness-events-page-limit">
                {page.data.limit}
              </Field>
              <Field label="more" testId="witness-events-more">
                {String(page.data.more)}
              </Field>
            </FieldGrid>
            <Table stack="lg" data-testid="witness-events-table">
              <TableHeader>
                <TableRow>
                  <TableHead>seq</TableHead>
                  <TableHead>payload_kind</TableHead>
                  <TableHead>event_id</TableHead>
                  <TableHead>prev</TableHead>
                  <TableHead>timestamp_ms</TableHead>
                  <TableHead>author_key</TableHead>
                  <TableHead>payload</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {page.data.events.map((event) => (
                  <TableRow key={event.event_id} data-testid={`witness-event-${event.seq}`}>
                    <TableCell label="seq" data-testid={`witness-event-seq-${event.seq}`}>
                      {event.seq}
                    </TableCell>
                    <TableCell
                      label="payload_kind"
                      data-testid={`witness-event-payload-kind-${event.seq}`}
                    >
                      {event.payload_kind}
                    </TableCell>
                    <TableCell label="event_id" data-testid={`witness-event-id-${event.seq}`}>
                      <Identifier value={event.event_id} />
                    </TableCell>
                    <TableCell label="prev" data-testid={`witness-event-prev-${event.seq}`}>
                      <Identifier value={event.prev} />
                    </TableCell>
                    <TableCell
                      label="timestamp_ms"
                      data-testid={`witness-event-timestamp-ms-${event.seq}`}
                    >
                      {event.timestamp_ms}
                    </TableCell>
                    <TableCell
                      label="author_key"
                      data-testid={`witness-event-author-key-${event.seq}`}
                    >
                      <Identifier value={event.author_key} />
                    </TableCell>
                    <TableCell
                      label="payload"
                      data-testid={`witness-event-payload-${event.seq}`}
                      className="break-all font-mono text-xs"
                    >
                      {JSON.stringify(event.payload)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <div className="flex gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={since === 0}
                data-testid="witness-events-previous"
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
                data-testid="witness-events-next"
                onClick={() => {
                  const next = since + limit;
                  setSince(next);
                  setSinceInput(String(next));
                }}
              >
                Next
              </Button>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
