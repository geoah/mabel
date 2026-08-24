import { type ReactNode, useState } from "react";

import { getIdentityLedger } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { EventLines } from "@/components/EventLines";
import { Section } from "@/components/Section";
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

/** How many entries one page holds. Nobody tunes this from the screen. */
const PAGE_SIZE = 8;

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
  const page = useResource(
    () => getIdentityLedger(identityId, { since, limit: PAGE_SIZE }),
    [identityId, since, version],
  );
  const held = page.data?.event_count ?? 0;
  // head_seq counts from zero, so a complete record holds head_seq + 1 entries.
  const total = (page.data?.head_seq ?? 0) + 1;
  const events = page.data?.events ?? [];
  const pageCount = Math.max(1, Math.ceil(total / PAGE_SIZE));
  const current = Math.floor(since / PAGE_SIZE) + 1;

  return (
    <Section
      testId="ledger-panel"
      title="Ledger"
      description="Everything this identity has signed, oldest first. Open a line to read the entry."
    >
        {page.loading && <p data-testid="ledger-loading">loading</p>}
        {page.error && <ErrorEnvelopeView error={page.error} testId="ledger-error" />}
        {page.data && held === 0 && (
          <>
            <p data-testid="ledger-not-fetched" className="text-sm">
              Your wallet holds none of this record&apos;s {page.data.head_seq + 1} entries yet.
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
                  Your wallet holds <span data-testid="ledger-event-count">{held}</span> of the{" "}
                  {total} entries on this record. Fetching asks a witness for the rest.
                </p>
                {fetch}
              </>
            )}
            {held >= total && (
              <p className="text-xs text-muted-foreground">
                <span data-testid="ledger-event-count">{held}</span>{" "}
                {held === 1 ? "entry" : "entries"}
              </p>
            )}
            {/* The footer: which page you are on, and nothing else. */}
            {pageCount > 1 && (
              <div data-testid="ledger-footer" className="border-t pt-3">
              <Pagination>
                <PaginationContent>
                  <PaginationItem>
                    <PaginationPrevious
                      data-testid="ledger-previous"
                      disabled={since === 0}
                      onClick={() => setSince(Math.max(0, since - PAGE_SIZE))}
                    />
                  </PaginationItem>
                  {pageNumbers(current, pageCount).map((shown, index) => (
                    <PaginationItem key={shown === "gap" ? `gap-${index}` : shown}>
                      {shown === "gap" ? (
                        <PaginationEllipsis />
                      ) : (
                        <PaginationLink
                          data-testid={`ledger-page-${shown}`}
                          isActive={shown === current}
                          onClick={() => setSince((shown - 1) * PAGE_SIZE)}
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
                      onClick={() => setSince(since + PAGE_SIZE)}
                    />
                  </PaginationItem>
                </PaginationContent>
              </Pagination>
              </div>
            )}
          </>
        )}
    </Section>
  );
}
