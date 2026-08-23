import { useState } from "react";

import { listLedgers } from "@/api/client";
import { DeclaredKindNote, DeclaredKindValue } from "@/components/DeclaredKind";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useResource } from "@/hooks/useResource";

import { WITNESS_HOLDINGS_NOTE } from "./notes";

/** One page of ledgers. The witness clamps limit at 256; this is a screenful. */
export const LEDGER_PAGE_SIZE = 4;

/**
 * GET /api/ledgers, ordered by ledger id and paged by offset. The kind column is
 * labelled "declared" because the kind is advisory (proposal 002 section 3).
 */
export function LedgerListPanel() {
  const [offset, setOffset] = useState(0);
  const page = useResource(() => listLedgers({ offset, limit: LEDGER_PAGE_SIZE }), [offset]);

  return (
    <Card data-testid="witness-ledger-list">
      <CardHeader>
        <CardTitle>Ledgers</CardTitle>
        <CardDescription data-testid="witness-holdings-note">
          {WITNESS_HOLDINGS_NOTE}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {page.loading && <p data-testid="witness-ledger-list-loading">loading</p>}
        {page.error && (
          <ErrorEnvelopeView error={page.error} testId="witness-ledger-list-error" />
        )}
        {page.data && page.data.entries.length === 0 && (
          <p data-testid="witness-ledger-list-empty">this witness holds no ledger</p>
        )}
        {page.data && page.data.entries.length > 0 && (
          <>
            <Table stack="xl" data-testid="witness-ledger-table">
              <TableHeader>
                <TableRow>
                  <TableHead>ledger_id</TableHead>
                  <TableHead>declared</TableHead>
                  <TableHead>head_seq</TableHead>
                  <TableHead>head_event</TableHead>
                  <TableHead>event_count</TableHead>
                  <TableHead>fork_count</TableHead>
                  <TableHead>first_seen_ms</TableHead>
                  <TableHead>updated_ms</TableHead>
                  <TableHead>source_endpoint</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {page.data.entries.map((entry) => (
                  <TableRow
                    key={entry.ledger_id}
                    data-testid={`witness-ledger-row-${entry.ledger_id}`}
                  >
                    <TableCell label="ledger_id">
                      <Identifier
                        value={entry.ledger_id}
                        to={`/witness/ledgers/${entry.ledger_id}`}
                        linkTestId={`witness-ledger-link-${entry.ledger_id}`}
                      />
                    </TableCell>
                    <TableCell label="declared">
                      <DeclaredKindValue
                        kind={entry.declared_kind}
                        testId={`witness-ledger-declared-kind-${entry.ledger_id}`}
                      />
                    </TableCell>
                    <TableCell
                      label="head_seq"
                      data-testid={`witness-ledger-head-seq-${entry.ledger_id}`}
                    >
                      {entry.head_seq}
                    </TableCell>
                    <TableCell
                      label="head_event"
                      data-testid={`witness-ledger-head-event-${entry.ledger_id}`}
                    >
                      <Identifier value={entry.head_event} />
                    </TableCell>
                    <TableCell
                      label="event_count"
                      data-testid={`witness-ledger-event-count-${entry.ledger_id}`}
                    >
                      {entry.event_count}
                    </TableCell>
                    <TableCell label="fork_count">
                      <span className="flex flex-wrap items-center gap-2">
                        <span data-testid={`witness-ledger-fork-count-${entry.ledger_id}`}>
                          {entry.fork_count}
                        </span>
                        {entry.forks_truncated ? (
                          <Badge
                            variant="outline"
                            data-testid={`witness-ledger-forks-truncated-${entry.ledger_id}`}
                          >
                            forks_truncated true
                          </Badge>
                        ) : (
                          <span
                            className="text-xs text-muted-foreground"
                            data-testid={`witness-ledger-forks-truncated-${entry.ledger_id}`}
                          >
                            forks_truncated false
                          </span>
                        )}
                      </span>
                    </TableCell>
                    <TableCell
                      label="first_seen_ms"
                      data-testid={`witness-ledger-first-seen-ms-${entry.ledger_id}`}
                    >
                      {entry.first_seen_ms}
                    </TableCell>
                    <TableCell
                      label="updated_ms"
                      data-testid={`witness-ledger-updated-ms-${entry.ledger_id}`}
                    >
                      {entry.updated_ms}
                    </TableCell>
                    <TableCell
                      label="source_endpoint"
                      data-testid={`witness-ledger-source-endpoint-${entry.ledger_id}`}
                    >
                      <Identifier value={entry.source_endpoint} />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
            <DeclaredKindNote testId="witness-ledger-declared-kind-note" />
            <p className="text-xs text-muted-foreground">
              forks_truncated says this witness stopped recording fork records for that ledger,
              so its fork_count is a floor
            </p>
            <div className="flex flex-wrap items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                disabled={offset === 0}
                data-testid="witness-ledger-previous"
                onClick={() => setOffset(Math.max(0, offset - LEDGER_PAGE_SIZE))}
              >
                Previous
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={!page.data.more}
                data-testid="witness-ledger-next"
                onClick={() => setOffset(offset + LEDGER_PAGE_SIZE)}
              >
                Next
              </Button>
              <span className="text-xs text-muted-foreground">
                <span data-testid="witness-ledger-offset">offset {page.data.offset}</span>
                {", "}
                <span data-testid="witness-ledger-limit">limit {page.data.limit}</span>
                {", "}
                <span data-testid="witness-ledger-more">more {String(page.data.more)}</span>
              </span>
            </div>
          </>
        )}
      </CardContent>
    </Card>
  );
}
