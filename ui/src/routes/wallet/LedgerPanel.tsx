import { useState } from "react";

import { getIdentityLedger } from "@/api/client";
import { DeclaredKindNote, DeclaredKindValue } from "@/components/DeclaredKind";
import { DeveloperOnly } from "@/components/DeveloperMode";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { EventLines } from "@/components/EventLines";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useResource } from "@/hooks/useResource";

const DEFAULT_LIMIT = 8;

/**
 * The ledger of one identity, one line per event. `?since=` is inclusive, so a
 * page starts at `seq === since`.
 */
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
        <CardDescription>
          One line per event: open a line for the event it records
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {page.loading && <p data-testid="ledger-loading">loading</p>}
        {page.error && <ErrorEnvelopeView error={page.error} testId="ledger-error" />}
        {page.data && (
          <>
            <EventLines events={page.data.events} />
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
