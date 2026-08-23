import { Link, useParams } from "react-router";

import { getLedger } from "@/api/client";
import { DeclaredKindNote, DeclaredKindValue } from "@/components/DeclaredKind";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Field, FieldGrid } from "@/components/Field";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

import { ForksPanel } from "./ForksPanel";
import { LedgerEventsPanel } from "./LedgerEventsPanel";
import { WITNESS_HOLDINGS_NOTE } from "./notes";

/** One ledger as this witness holds it: the summary, its events and its forks. */
export function WitnessLedgerDetail() {
  const { ledgerId = "" } = useParams();
  const ledger = useResource(() => getLedger(ledgerId), [ledgerId]);

  return (
    <div className="space-y-4">
      <Link to="/witness" className="text-sm underline" data-testid="witness-ledger-back">
        Ledgers
      </Link>
      {ledger.loading && <p data-testid="witness-ledger-detail-loading">loading</p>}
      {ledger.error && (
        <ErrorEnvelopeView error={ledger.error} testId="witness-ledger-detail-error" />
      )}
      {ledger.data && (
        <>
          <Card data-testid="witness-ledger-detail">
            <CardHeader>
              <CardTitle className="break-all font-mono text-sm">
                {ledger.data.entry.ledger_id}
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              <FieldGrid>
                <Field label="ledger_id" testId="witness-detail-ledger-id" mono>
                  {ledger.data.entry.ledger_id}
                </Field>
                <Field label="declared" testId="witness-detail-declared-kind-row">
                  <DeclaredKindValue
                    kind={ledger.data.entry.declared_kind}
                    testId="witness-detail-declared-kind"
                  />
                </Field>
                <Field label="head_seq" testId="witness-detail-head-seq">
                  {ledger.data.entry.head_seq}
                </Field>
                <Field label="head_event" testId="witness-detail-head-event" mono>
                  {ledger.data.entry.head_event}
                </Field>
                <Field label="event_count" testId="witness-detail-event-count">
                  {ledger.data.entry.event_count}
                </Field>
                <Field label="fork_count" testId="witness-detail-fork-count">
                  {ledger.data.entry.fork_count}
                </Field>
                <Field label="forks_truncated" testId="witness-detail-forks-truncated">
                  {ledger.data.entry.forks_truncated ? (
                    <Badge variant="outline">true</Badge>
                  ) : (
                    "false"
                  )}
                </Field>
                <Field label="first_seen_ms" testId="witness-detail-first-seen-ms">
                  {ledger.data.entry.first_seen_ms}
                </Field>
                <Field label="updated_ms" testId="witness-detail-updated-ms">
                  {ledger.data.entry.updated_ms}
                </Field>
                <Field label="source_endpoint" testId="witness-detail-source-endpoint" mono>
                  {ledger.data.entry.source_endpoint}
                </Field>
                <Field label="witnesses" testId="witness-detail-witnesses" mono>
                  {ledger.data.witnesses.length === 0
                    ? "none"
                    : ledger.data.witnesses.join(", ")}
                </Field>
              </FieldGrid>
              <DeclaredKindNote testId="witness-detail-declared-kind-note" />
              <p className="text-xs text-muted-foreground" data-testid="witness-detail-holdings-note">
                {WITNESS_HOLDINGS_NOTE}
              </p>
            </CardContent>
          </Card>
          <LedgerEventsPanel ledgerId={ledger.data.entry.ledger_id} />
          <ForksPanel ledgerId={ledger.data.entry.ledger_id} />
        </>
      )}
    </div>
  );
}
