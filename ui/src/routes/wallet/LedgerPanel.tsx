import { type ReactNode, useState } from "react";

import { getIdentityLedger } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { EventLines } from "@/components/EventLines";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Pagination,
  PaginationContent,
  PaginationEllipsis,
  PaginationItem,
  PaginationLink,
  PaginationNext,
  PaginationPrevious,
} from "@/components/ui/pagination";
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
 * Which page numbers the bar draws: the first, the last, and the current one
 * with its neighbours. A gap between two of them is an ellipsis. The numbers are
 * one-based, because a page number is not a position on the record.
 */
function pageNumbers(current: number, count: number): (number | "gap")[] {
  const wanted = new Set([1, count, current, current - 1, current + 1]);
  const shown = [...wanted].filter((page) => page >= 1 && page <= count).sort((a, b) => a - b);
  const drawn: (number | "gap")[] = [];
  let previous = 0;
  for (const page of shown) {
    if (previous !== 0 && page - previous > 1) {
      drawn.push("gap");
    }
    drawn.push(page);
    previous = page;
  }
  return drawn;
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
  const pageCount = Math.max(1, Math.ceil(total / Math.max(1, limit)));

  /** Moves to a position, and says so in the box that names one. */
  function goTo(position: number) {
    setSince(position);
    setSinceInput(String(position));
  }

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
            {/* The footer: which page you are on, and where that page sits. */}
            <div data-testid="ledger-footer" className="space-y-2 border-t pt-3">
              <Pagination>
                <PaginationContent>
                  <PaginationItem>
                    <PaginationPrevious
                      data-testid="ledger-previous"
                      disabled={since === 0}
                      onClick={() => goTo(Math.max(0, since - limit))}
                    />
                  </PaginationItem>
                  {pageNumbers(Math.floor(since / limit) + 1, pageCount).map((shown, index) => (
                    <PaginationItem key={shown === "gap" ? `gap-${index}` : shown}>
                      {shown === "gap" ? (
                        <PaginationEllipsis />
                      ) : (
                        <PaginationLink
                          data-testid={`ledger-page-${shown}`}
                          isActive={shown === Math.floor(since / limit) + 1}
                          onClick={() => goTo((shown - 1) * limit)}
                        >
                          {shown}
                        </PaginationLink>
                      )}
                    </PaginationItem>
                  ))}
                  <PaginationItem>
                    <PaginationNext
                      data-testid="ledger-next"
                      disabled={!page.data.more}
                      onClick={() => goTo(since + limit)}
                    />
                  </PaginationItem>
                </PaginationContent>
              </Pagination>
              <p data-testid="ledger-range" className="text-xs text-muted-foreground">
                {pageRange(events[0].seq, events[events.length - 1].seq, total)}
              </p>
              <div className="flex flex-wrap items-end gap-2">
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
