import { type FormEvent, useState } from "react";

import { type ApiError, syncPush } from "@/api/client";
import type { SyncPushResponse } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Nullable } from "@/components/Field";
import { InlineField, InlineForm } from "@/components/InlineForm";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Identifier } from "@/components/Identifier";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
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
 * Handing the record to the witnesses. One witness accepting is enough to
 * succeed, and the table reports what every one of them said; a send where all
 * of them failed answers code 30.
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
    <div data-testid="sync-push" className="space-y-3">
      <InlineForm onSubmit={submit} data-testid="sync-push-form">
        <InlineField label="One witness only (optional)" htmlFor="sync-push-to">
          <Input
            id="sync-push-to"
            data-testid="sync-push-to"
            value={to}
            onChange={(event) => setTo(event.target.value)}
            placeholder="leave empty to send to all of them"
            className="font-mono text-xs"
          />
        </InlineField>
        <Button type="submit" data-testid="sync-push-submit" disabled={pending}>
          {pending ? "sending" : "Send"}
        </Button>
      </InlineForm>
      {error && <ErrorEnvelopeView error={error} testId="sync-push-error" />}
      {report && (
        <div className="space-y-2" data-testid="sync-push-report">
          <KeyValueTable>
            <KeyValue label="identity" testId="sync-push-ledger-id">
              <Identifier value={report.ledger_id} />
            </KeyValue>
            <KeyValue label="newest position" testId="sync-push-head-seq">
              {report.head_seq}
            </KeyValue>
            <KeyValue label="newest entry" testId="sync-push-head-event">
              <Identifier value={report.head_event} />
            </KeyValue>
          </KeyValueTable>
          <Table stack="lg" data-testid="sync-push-results">
            <TableHeader>
              <TableRow>
                <TableHead>witness</TableHead>
                <TableHead>what it said</TableHead>
                <TableHead>its newest position</TableHead>
                <TableHead>entries it stored</TableHead>
                <TableHead>why it refused</TableHead>
                <TableHead>at position</TableHead>
                <TableHead>detail</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {report.results.map((result) => (
                <TableRow
                  key={result.endpoint}
                  data-testid={`sync-push-result-${result.endpoint}`}
                >
                  <TableCell label="witness">
                    <Identifier value={result.endpoint} />
                  </TableCell>
                  <TableCell label="what it said" data-testid={`push-status-${result.endpoint}`}>
                    {result.status}
                  </TableCell>
                  <TableCell label="its newest position" data-testid={`push-head-seq-${result.endpoint}`}>
                    <Nullable value={result.head_seq} />
                  </TableCell>
                  <TableCell label="entries it stored" data-testid={`push-stored-${result.endpoint}`}>
                    {result.stored}
                  </TableCell>
                  <TableCell
                    label="why it refused"
                    data-testid={`push-reject-code-${result.endpoint}`}
                  >
                    <Nullable value={result.reject_code} />
                  </TableCell>
                  <TableCell label="at position" data-testid={`push-at-seq-${result.endpoint}`}>
                    <Nullable value={result.at_seq} />
                  </TableCell>
                  <TableCell
                    label="detail"
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
    </div>
  );
}
