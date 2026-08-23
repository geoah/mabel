import { type FormEvent, useState } from "react";

import { type ApiError, verifyLedger, verifyTrust } from "@/api/client";
import type { VerifyReport } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { asApiError } from "@/hooks/useResource";

import { VerifyReportView } from "./VerifyReportView";

export function VerifyPage() {
  const [issuer, setIssuer] = useState("");
  const [subject, setSubject] = useState("");
  const [trustFrom, setTrustFrom] = useState("");
  const [ledgerId, setLedgerId] = useState("");
  const [ledgerFrom, setLedgerFrom] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [report, setReport] = useState<VerifyReport | null>(null);

  async function run(produce: () => Promise<VerifyReport>) {
    setPending(true);
    setError(null);
    setReport(null);
    try {
      setReport(await produce());
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  function submitTrust(event: FormEvent) {
    event.preventDefault();
    void run(() =>
      verifyTrust({
        kind: "trust",
        issuer: issuer.trim(),
        subject: subject.trim(),
        from: trustFrom.trim() || null,
      }),
    );
  }

  function submitLedger(event: FormEvent) {
    event.preventDefault();
    void run(() =>
      verifyLedger({
        kind: "ledger",
        ledger_id: ledgerId.trim(),
        from: ledgerFrom.trim() || null,
      }),
    );
  }

  return (
    <div className="space-y-4">
      <div className="grid gap-4 lg:grid-cols-2">
        <Card data-testid="verify-trust">
          <CardHeader>
            <CardTitle>Verify trust</CardTitle>
            <CardDescription>
              Answers whether the issuer holds an unrevoked attestation for the subject
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={submitTrust} className="space-y-2" data-testid="verify-trust-form">
              <div className="space-y-1">
                <Label htmlFor="verify-trust-issuer">issuer</Label>
                <Input
                  id="verify-trust-issuer"
                  data-testid="verify-trust-issuer"
                  value={issuer}
                  onChange={(event) => setIssuer(event.target.value)}
                />
              </div>
              <div className="space-y-1">
                <Label htmlFor="verify-trust-subject">subject</Label>
                <Input
                  id="verify-trust-subject"
                  data-testid="verify-trust-subject"
                  value={subject}
                  onChange={(event) => setSubject(event.target.value)}
                />
              </div>
              <div className="space-y-1">
                <Label htmlFor="verify-trust-from">from (optional source)</Label>
                <Input
                  id="verify-trust-from"
                  data-testid="verify-trust-from"
                  value={trustFrom}
                  onChange={(event) => setTrustFrom(event.target.value)}
                />
              </div>
              <Button type="submit" data-testid="verify-trust-submit" disabled={pending}>
                Verify trust
              </Button>
            </form>
          </CardContent>
        </Card>

        <Card data-testid="verify-ledger">
          <CardHeader>
            <CardTitle>Verify ledger</CardTitle>
            <CardDescription>
              Partial validity is a failure: the node answers code 20 with the report in details
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form onSubmit={submitLedger} className="space-y-2" data-testid="verify-ledger-form">
              <div className="space-y-1">
                <Label htmlFor="verify-ledger-id">ledger_id</Label>
                <Input
                  id="verify-ledger-id"
                  data-testid="verify-ledger-id"
                  value={ledgerId}
                  onChange={(event) => setLedgerId(event.target.value)}
                />
              </div>
              <div className="space-y-1">
                <Label htmlFor="verify-ledger-from">from (optional source)</Label>
                <Input
                  id="verify-ledger-from"
                  data-testid="verify-ledger-from"
                  value={ledgerFrom}
                  onChange={(event) => setLedgerFrom(event.target.value)}
                />
              </div>
              <Button type="submit" data-testid="verify-ledger-submit" disabled={pending}>
                Verify ledger
              </Button>
            </form>
          </CardContent>
        </Card>
      </div>
      {error && <ErrorEnvelopeView error={error} testId="verify-error" />}
      {report && <VerifyReportView report={report} />}
    </div>
  );
}
