import { useState } from "react";

import { getIdentityLedger } from "@/api/client";
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

const DEFAULT_LIMIT = 8;

/** ?since= is inclusive: the page starts at seq === since. */
export function LedgerPanel({ identityId, version }: { identityId: string; version: number }) {
  const [since, setSince] = useState(0);
  const [limit, setLimit] = useState(DEFAULT_LIMIT);
  const [sinceInput, setSinceInput] = useState("0");
  const [limitInput, setLimitInput] = useState(String(DEFAULT_LIMIT));
  const page = useResource(
    () => getIdentityLedger(identityId, { since, limit }),
    [identityId, since, limit, version],
  );

  return (
    <Card data-testid="ledger-panel">
      <CardHeader>
        <CardTitle>Ledger</CardTitle>
        <CardDescription>since is inclusive: the page starts at seq equal to since</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <div className="flex flex-wrap items-end gap-2">
          <div className="space-y-1">
            <Label htmlFor="ledger-since">since</Label>
            <Input
              id="ledger-since"
              data-testid="ledger-since"
              value={sinceInput}
              onChange={(event) => setSinceInput(event.target.value)}
              className="w-24"
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="ledger-limit">limit</Label>
            <Input
              id="ledger-limit"
              data-testid="ledger-limit"
              value={limitInput}
              onChange={(event) => setLimitInput(event.target.value)}
              className="w-24"
            />
          </div>
          <Button
            variant="outline"
            data-testid="ledger-load"
            onClick={() => {
              setSince(Number(sinceInput));
              setLimit(Number(limitInput) || DEFAULT_LIMIT);
            }}
          >
            Load
          </Button>
        </div>
        {page.loading && <p data-testid="ledger-loading">loading</p>}
        {page.error && <ErrorEnvelopeView error={page.error} testId="ledger-error" />}
        {page.data && (
          <>
            <KeyValueTable>
              <KeyValue label="declared_kind" testId="ledger-declared-kind-row">
                <DeclaredKindValue
                  kind={page.data.declared_kind}
                  testId="ledger-declared-kind"
                />
              </KeyValue>
              <KeyValue label="since" testId="ledger-page-since">
                {page.data.since}
              </KeyValue>
              <KeyValue label="head_seq" testId="ledger-head-seq">
                {page.data.head_seq}
              </KeyValue>
              <KeyValue label="event_count" testId="ledger-event-count">
                {page.data.event_count}
              </KeyValue>
              <KeyValue label="more" testId="ledger-more">
                {String(page.data.more)}
              </KeyValue>
              <DeveloperOnly>
                <KeyValue label="ledger_id" testId="ledger-id">
                  <Identifier value={page.data.ledger_id} />
                </KeyValue>
                <KeyValue label="head_event" testId="ledger-head-event">
                  <Identifier value={page.data.head_event} />
                </KeyValue>
              </DeveloperOnly>
            </KeyValueTable>
            <DeclaredKindNote testId="ledger-declared-kind-note" />
            <Table stack="lg" data-testid="ledger-events">
              <TableHeader>
                <TableRow>
                  <TableHead>seq</TableHead>
                  <TableHead>payload_kind</TableHead>
                  <TableHead>event_id</TableHead>
                  <TableHead>prev</TableHead>
                  <TableHead>timestamp_ms</TableHead>
                  <TableHead>payload</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {page.data.events.map((event) => (
                  <TableRow key={event.event_id} data-testid={`ledger-event-${event.seq}`}>
                    <TableCell label="seq" data-testid={`event-seq-${event.seq}`}>
                      {event.seq}
                    </TableCell>
                    <TableCell
                      label="payload_kind"
                      data-testid={`event-payload-kind-${event.seq}`}
                    >
                      {event.payload_kind}
                    </TableCell>
                    <TableCell label="event_id" data-testid={`event-id-${event.seq}`}>
                      <Identifier value={event.event_id} />
                    </TableCell>
                    <TableCell label="prev" data-testid={`event-prev-${event.seq}`}>
                      <Identifier value={event.prev} />
                    </TableCell>
                    <TableCell
                      label="timestamp_ms"
                      data-testid={`event-timestamp-ms-${event.seq}`}
                    >
                      {event.timestamp_ms}
                    </TableCell>
                    <TableCell
                      label="payload"
                      data-testid={`event-payload-${event.seq}`}
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
          </>
        )}
      </CardContent>
    </Card>
  );
}
