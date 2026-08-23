import { type FormEvent, useState } from "react";

import { addTrust, type ApiError, revokeTrust } from "@/api/client";
import type { Identity } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { Badge } from "@/components/ui/badge";
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

export function TrustPanel({
  identity,
  onAppended,
}: {
  identity: Identity;
  onAppended: () => void;
}) {
  const [subject, setSubject] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [appended, setAppended] = useState<string | null>(null);

  async function add(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    setAppended(null);
    try {
      const response = await addTrust({
        issuer: identity.identity_id,
        subject: subject.trim(),
      });
      setAppended(response.event.event_id);
      setSubject("");
      onAppended();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  async function revoke(attestationEvent: string) {
    setPending(true);
    setError(null);
    setAppended(null);
    try {
      const response = await revokeTrust(attestationEvent, { issuer: identity.identity_id });
      setAppended(response.event.event_id);
      onAppended();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <Card data-testid="trust-panel">
      <CardHeader>
        <CardTitle>Trust</CardTitle>
        <CardDescription>
          One unrevoked attestation per subject; revoking names the attestation event
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <form onSubmit={add} className="space-y-2" data-testid="trust-add-form">
          <div className="space-y-1">
            <Label htmlFor="trust-add-subject">subject</Label>
            <Input
              id="trust-add-subject"
              data-testid="trust-add-subject"
              value={subject}
              onChange={(event) => setSubject(event.target.value)}
              placeholder="identity id of the subject"
            />
          </div>
          <Button type="submit" data-testid="trust-add-submit" disabled={pending}>
            {pending ? "appending" : "Attest trust"}
          </Button>
        </form>
        {appended && (
          <p data-testid="trust-appended-event">
            <Identifier value={appended} />
          </p>
        )}
        {error && <ErrorEnvelopeView error={error} testId="trust-error" />}
        {identity.trust.length === 0 ? (
          <p data-testid="trust-list-empty" className="text-sm">
            no attestations in this ledger
          </p>
        ) : (
          <Table stack="md" data-testid="trust-list">
            <TableHeader>
              <TableRow>
                <TableHead>subject</TableHead>
                <TableHead>attestation_seq</TableHead>
                <TableHead>state</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {identity.trust.map((record) => (
                <TableRow
                  key={record.attestation_event}
                  data-testid={`trust-row-${record.attestation_event}`}
                >
                  <TableCell label="subject">
                    <Identifier value={record.subject} />
                  </TableCell>
                  <TableCell
                    label="attestation_seq"
                    data-testid={`trust-attestation-seq-${record.attestation_event}`}
                  >
                    {record.attestation_seq}
                  </TableCell>
                  <TableCell label="state">
                    <Badge
                      variant={record.revoked ? "destructive" : "secondary"}
                      data-testid={`trust-state-${record.attestation_event}`}
                    >
                      {record.revoked
                        ? `revoked at seq ${record.revocation_seq}`
                        : "unrevoked"}
                    </Badge>
                  </TableCell>
                  <TableCell>
                    <Button
                      variant="outline"
                      size="sm"
                      disabled={pending || record.revoked}
                      onClick={() => revoke(record.attestation_event)}
                      data-testid={`trust-revoke-${record.attestation_event}`}
                    >
                      Revoke
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        )}
      </CardContent>
    </Card>
  );
}
