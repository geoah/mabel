import { useState } from "react";

import { type ApiError, forceVerification } from "@/api/client";
import type { Identity, Verification } from "@/api/types";
import { DeveloperOnly } from "@/components/DeveloperMode";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { VerificationMark, VerificationNote } from "@/components/ResolvedIdentity";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { asApiError } from "@/hooks/useResource";

/**
 * The advisory DNS verdict for the hostname this identity claims. Checking is
 * manual: the GET routes answer from the cache, and this button forces one
 * check and waits for it (proposal 003 section 2).
 */
export function VerificationPanel({
  identity,
  onChecked,
}: {
  identity: Identity;
  onChecked: () => void;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [checked, setChecked] = useState<Verification | null>(null);
  const verification = checked ?? identity.verification;

  async function check() {
    setPending(true);
    setError(null);
    try {
      const response = await forceVerification(identity.identity_id);
      setChecked(response.verification);
      onChecked();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <Card data-testid="verification-panel">
      <CardHeader>
        <CardTitle>Hostname</CardTitle>
        <CardDescription>
          A TXT record at _mabel.&lt;hostname&gt; whose value is mabel=&lt;identity id&gt;
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <KeyValueTable>
          <KeyValue label="status" testId="verification-status">
            {verification.status === "unclaimed" || verification.hostname === null ? (
              "unclaimed"
            ) : (
              <VerificationMark
                status={verification.status}
                hostname={verification.hostname}
                stale={verification.stale}
                testId="verification-mark"
              />
            )}
          </KeyValue>
          <DeveloperOnly>
            <KeyValue label="checked_at_ms" testId="verification-checked-at-ms">
              {verification.checked_at_ms ?? "null"}
            </KeyValue>
            <KeyValue label="last_verified_at_ms" testId="verification-last-verified-at-ms">
              {verification.last_verified_at_ms ?? "null"}
            </KeyValue>
            <KeyValue label="stale" testId="verification-stale">
              {String(verification.stale)}
            </KeyValue>
            <KeyValue label="detail" testId="verification-detail">
              <span className="font-mono text-xs">{verification.detail ?? "null"}</span>
            </KeyValue>
            <KeyValue label="unreachable" testId="verification-unreachable">
              {verification.unreachable === null
                ? "null"
                : `${verification.unreachable.checked_at_ms}: ${verification.unreachable.detail ?? "null"}`}
            </KeyValue>
          </DeveloperOnly>
        </KeyValueTable>
        <Button
          variant="outline"
          size="sm"
          data-testid="verification-check"
          disabled={pending}
          onClick={() => void check()}
        >
          {pending ? "checking" : "Check now"}
        </Button>
        {error && <ErrorEnvelopeView error={error} testId="verification-error" />}
        <VerificationNote testId="verification-note" />
      </CardContent>
    </Card>
  );
}
