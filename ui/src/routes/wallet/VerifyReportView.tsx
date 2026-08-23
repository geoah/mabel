import type { VerifyReport } from "@/api/types";
import { DeclaredKindNote, DeclaredKindValue } from "@/components/DeclaredKind";
import { Nullable } from "@/components/Field";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Identifier } from "@/components/Identifier";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";

/**
 * The verification report. `statement` is the flag-R sentence the node renders,
 * printed verbatim: the UI never composes its own "as of seq N from source S".
 */
export function VerifyReportView({ report }: { report: VerifyReport }) {
  return (
    <Card data-testid="verify-report">
      <CardHeader>
        <CardTitle>Report</CardTitle>
      </CardHeader>
      <CardContent className="space-y-3">
        <p data-testid="verify-report-statement" className="text-sm">
          {report.statement}
        </p>
        <KeyValueTable>
          <KeyValue label="kind" testId="verify-report-kind">
            {report.kind}
          </KeyValue>
          <KeyValue label="source" testId="verify-report-source">
            <Identifier value={report.source} full />
          </KeyValue>
          <KeyValue label="sources_queried" testId="verify-report-sources-queried">
            <span className="flex flex-col gap-1">
              {report.sources_queried.map((source) => (
                <Identifier key={source} value={source} full />
              ))}
            </span>
          </KeyValue>
          <KeyValue label="head_seq" testId="verify-report-head-seq">
            {report.head_seq}
          </KeyValue>
          <KeyValue label="head_event" testId="verify-report-head-event">
            <Identifier value={report.head_event} full />
          </KeyValue>
          <KeyValue label="fetched_at_ms" testId="verify-report-fetched-at-ms">
            {report.fetched_at_ms}
          </KeyValue>
          {report.signing_principal !== undefined && (
            <KeyValue label="signing_principal" testId="verify-report-signing-principal">
              {report.signing_principal === null ? (
                <Identifier value={null} full />
              ) : (
                <span className="flex flex-col gap-1">
                  <Identifier value={report.signing_principal.identity} full />
                  <span className="text-muted-foreground text-xs">
                    key <Identifier value={report.signing_principal.key} full />
                  </span>
                </span>
              )}
            </KeyValue>
          )}
          {report.kind === "trust" ? (
            <TrustFields report={report} />
          ) : (
            <LedgerFields report={report} />
          )}
        </KeyValueTable>
        {report.kind === "ledger" && (
          <DeclaredKindNote testId="verify-report-declared-kind-note" />
        )}
        {report.kind === "trust" && report.revoked_attestations.length > 0 && (
          <Table stack="md" data-testid="verify-report-revoked-attestations">
            <TableHeader>
              <TableRow>
                <TableHead>attestation_event</TableHead>
                <TableHead>attestation_seq</TableHead>
                <TableHead>revocation_event</TableHead>
                <TableHead>revocation_seq</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {report.revoked_attestations.map((revoked) => (
                <TableRow
                  key={revoked.attestation_event}
                  data-testid={`verify-report-revoked-${revoked.attestation_event}`}
                >
                  <TableCell label="attestation_event">
                    <Identifier value={revoked.attestation_event} full />
                  </TableCell>
                  <TableCell label="attestation_seq">{revoked.attestation_seq}</TableCell>
                  <TableCell label="revocation_event">
                    <Identifier value={revoked.revocation_event} full />
                  </TableCell>
                  <TableCell label="revocation_seq">{revoked.revocation_seq}</TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
        {report.kind === "trust" && (
          <p data-testid="verify-report-subject-control" className="text-xs">
            {report.subject_control}
          </p>
        )}
        <p data-testid="verify-report-verified-means" className="text-xs">
          {report.verified_means}
        </p>
      </CardContent>
    </Card>
  );
}

function TrustFields({ report }: { report: Extract<VerifyReport, { kind: "trust" }> }) {
  return (
    <>
      <KeyValue label="trusted" testId="verify-report-trusted">
        <Badge
          variant={report.trusted ? "secondary" : "destructive"}
          data-testid="verify-report-trusted-badge"
        >
          {String(report.trusted)}
        </Badge>
      </KeyValue>
      <KeyValue label="issuer" testId="verify-report-issuer">
        <Identifier value={report.issuer} full />
      </KeyValue>
      <KeyValue label="subject" testId="verify-report-subject">
        <Identifier value={report.subject} full />
      </KeyValue>
      <KeyValue label="subject_resolution" testId="verify-report-subject-resolution">
        {report.subject_resolution}
      </KeyValue>
      <KeyValue label="subject_note" testId="verify-report-subject-note">
        <Nullable value={report.subject_note} />
      </KeyValue>
      <KeyValue label="attestation_event" testId="verify-report-attestation-event">
        <Identifier value={report.attestation_event} full />
      </KeyValue>
      <KeyValue label="attestation_seq" testId="verify-report-attestation-seq">
        <Nullable value={report.attestation_seq} />
      </KeyValue>
      <KeyValue label="revoked_count" testId="verify-report-revoked-count">
        {report.revoked_count}
      </KeyValue>
    </>
  );
}

function LedgerFields({ report }: { report: Extract<VerifyReport, { kind: "ledger" }> }) {
  return (
    <>
      <KeyValue label="ledger_id" testId="verify-report-ledger-id">
        <Identifier value={report.ledger_id} full />
      </KeyValue>
      <KeyValue label="declared_kind" testId="verify-report-declared-kind-row">
        <DeclaredKindValue
          kind={report.declared_kind}
          testId="verify-report-declared-kind"
        />
      </KeyValue>
      <KeyValue label="valid" testId="verify-report-valid">
        {String(report.valid)}
      </KeyValue>
      <KeyValue label="valid_to_seq" testId="verify-report-valid-to-seq">
        {report.valid_to_seq}
      </KeyValue>
      <KeyValue label="failed_at_seq" testId="verify-report-failed-at-seq">
        <Nullable value={report.failed_at_seq} />
      </KeyValue>
      <KeyValue label="event_count" testId="verify-report-event-count">
        {report.event_count}
      </KeyValue>
    </>
  );
}
