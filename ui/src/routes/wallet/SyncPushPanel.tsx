import { type FormEvent, useState } from "react";

import { type ApiError, syncPush } from "@/api/client";
import type { SyncPushResponse } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Field, FieldGrid, Nullable } from "@/components/Field";
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
import { asApiError } from "@/hooks/useResource";

/**
 * A push where at least one witness accepted succeeds and reports the failures
 * per endpoint; a push where every witness failed answers code 30.
 */
export function SyncPushPanel({ identityId }: { identityId: string }) {
  const [to, setTo] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [report, setReport] = useState<SyncPushResponse | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    setReport(null);
    try {
      setReport(await syncPush({ identity_id: identityId, to: to.trim() || null }));
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <Card data-testid="sync-push">
      <CardHeader>
        <CardTitle>Push</CardTitle>
        <CardDescription>Empty target pushes to every configured witness</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <form onSubmit={submit} className="space-y-2" data-testid="sync-push-form">
          <div className="space-y-1">
            <Label htmlFor="sync-push-to">to (optional endpoint id)</Label>
            <Input
              id="sync-push-to"
              data-testid="sync-push-to"
              value={to}
              onChange={(event) => setTo(event.target.value)}
            />
          </div>
          <Button type="submit" data-testid="sync-push-submit" disabled={pending}>
            {pending ? "pushing" : "Push"}
          </Button>
        </form>
        {error && <ErrorEnvelopeView error={error} testId="sync-push-error" />}
        {report && (
          <div className="space-y-2" data-testid="sync-push-report">
            <FieldGrid>
              <Field label="ledger_id" testId="sync-push-ledger-id" mono>
                {report.ledger_id}
              </Field>
              <Field label="head_seq" testId="sync-push-head-seq">
                {report.head_seq}
              </Field>
              <Field label="head_event" testId="sync-push-head-event" mono>
                {report.head_event}
              </Field>
            </FieldGrid>
            <Table data-testid="sync-push-results">
              <TableHeader>
                <TableRow>
                  <TableHead>endpoint</TableHead>
                  <TableHead>status</TableHead>
                  <TableHead>head_seq</TableHead>
                  <TableHead>stored</TableHead>
                  <TableHead>reject_code</TableHead>
                  <TableHead>at_seq</TableHead>
                  <TableHead>message</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {report.results.map((result) => (
                  <TableRow
                    key={result.endpoint}
                    data-testid={`sync-push-result-${result.endpoint}`}
                  >
                    <TableCell className="break-all font-mono text-xs">
                      {result.endpoint}
                    </TableCell>
                    <TableCell data-testid={`push-status-${result.endpoint}`}>
                      {result.status}
                    </TableCell>
                    <TableCell data-testid={`push-head-seq-${result.endpoint}`}>
                      <Nullable value={result.head_seq} />
                    </TableCell>
                    <TableCell data-testid={`push-stored-${result.endpoint}`}>
                      {result.stored}
                    </TableCell>
                    <TableCell data-testid={`push-reject-code-${result.endpoint}`}>
                      <Nullable value={result.reject_code} />
                    </TableCell>
                    <TableCell data-testid={`push-at-seq-${result.endpoint}`}>
                      <Nullable value={result.at_seq} />
                    </TableCell>
                    <TableCell
                      data-testid={`push-message-${result.endpoint}`}
                      className="text-xs"
                    >
                      <Nullable value={result.message} />
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
