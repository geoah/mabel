import { type ReactNode, useState } from "react";

import { getIdentityLedger } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { EventLines } from "@/components/EventLines";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useResource } from "@/hooks/useResource";

const DEFAULT_LIMIT = 8;

/**
 * Where the page a reader is looking at sits in the whole ledger. Positions are
 * counted from zero on the record itself, so the footer names positions rather
 * than inventing a one-based entry number the ids would disagree with.
 */
function pageRange(first: number, last: number, total: number): string {
  return `Showing positions ${first} to ${last} of ${total}.`;
}

/**
 * The ledger of one identity, one compact row per entry. `?since=` is inclusive,
 * so a page starts at `seq === since`.
 *
 * A summary can arrive without the entries behind it: the node answers a head
 * position for a ledger it knows of and no events for one it never fetched. The
 * panel says which of the two it is holding rather than printing zero entries
 * against a head position that is not zero (decision 017).
 */
export function LedgerPanel({
  identityId,
  version,
  fetch,
}: {
  identityId: string;
  version: number;
  /** A control that asks a witness for the missing entries, when one applies. */
  fetch?: ReactNode;
}) {
  const [since, setSince] = useState(0);
  const [limit, setLimit] = useState(DEFAULT_LIMIT);
  const [sinceInput, setSinceInput] = useState("0");
  const [limitInput, setLimitInput] = useState(String(DEFAULT_LIMIT));
  const page = useResource(
    () => getIdentityLedger(identityId, { since, limit }),
    [identityId, since, limit, version],
  );
  const held = page.data?.event_count ?? 0;
  // head_seq counts from zero, so a complete record holds head_seq + 1 entries.
  const total = (page.data?.head_seq ?? 0) + 1;
  const events = page.data?.events ?? [];

  return (
    <Card data-testid="ledger-panel">
      <CardHeader>
        <CardTitle>Ledger</CardTitle>
        <CardDescription>
          Everything this identity has signed, oldest first. Open a line to read the entry.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {page.loading && <p data-testid="ledger-loading">loading</p>}
        {page.error && <ErrorEnvelopeView error={page.error} testId="ledger-error" />}
        {page.data && held === 0 && (
          <>
            <p data-testid="ledger-not-fetched" className="text-sm">
              Your wallet knows this record reaches position {page.data.head_seq} but has not
              fetched any of its entries.
            </p>
            {fetch}
          </>
        )}
        {page.data && held > 0 && (
          <>
            <EventLines events={events} />
            {held < total && (
              <>
                <p data-testid="ledger-partial" className="text-sm">
                  Your wallet holds{" "}
                  <span data-testid="ledger-event-count">{held}</span> of the {total} entries on
                  this record, the newest at position{" "}
                  <span data-testid="ledger-head-seq">{page.data.head_seq}</span>. Fetching again
                  asks a witness for the rest.
                </p>
                {fetch}
              </>
            )}
            {held >= total && (
              <p className="text-xs text-muted-foreground">
                <span data-testid="ledger-event-count">{held}</span>{" "}
                {held === 1 ? "entry" : "entries"} on this ledger, the newest at position{" "}
                <span data-testid="ledger-head-seq">{page.data.head_seq}</span>.
              </p>
            )}
            {/* The footer: where you are, and the two buttons that move. */}
            <div
              data-testid="ledger-footer"
              className="flex flex-wrap items-end gap-x-4 gap-y-2 border-t pt-3"
            >
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
              <p data-testid="ledger-range" className="text-xs text-muted-foreground">
                {pageRange(events[0].seq, events[events.length - 1].seq, total)}
              </p>
              <div className="flex items-end gap-2">
                <div className="space-y-1">
                  <Label htmlFor="ledger-since">from position</Label>
                  <Input
                    id="ledger-since"
                    data-testid="ledger-since"
                    value={sinceInput}
                    onChange={(event) => setSinceInput(event.target.value)}
                    className="w-20"
                  />
                </div>
                <div className="space-y-1">
                  <Label htmlFor="ledger-limit">how many</Label>
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
          </>
        )}
      </CardContent>
    </Card>
  );
}
