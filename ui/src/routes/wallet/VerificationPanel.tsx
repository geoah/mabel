import { useState } from "react";

import { type ApiError, forceVerification } from "@/api/client";
import type { Identity, Verification } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { VerificationMark } from "@/components/identity";
import { Button } from "@/components/ui/button";
import { asApiError } from "@/hooks/useResource";
import { formatTimestamp } from "@/lib/time";

/**
 * Whether the handle this identity claims names it back in DNS. Checking is
 * manual: the GET routes answer from the cache, and this button forces one
 * check and waits for it (proposal 003 section 2). The verdict is advisory and
 * gates nothing (decision 015).
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
    <div data-testid="verification-panel" className="space-y-3">
      <KeyValueTable>
        <KeyValue label="handle" testId="verification-status">
          {verification.status === "unclaimed" || verification.hostname === null ? (
            "this identity claims no handle"
          ) : (
            <VerificationMark
              status={verification.status}
              hostname={verification.hostname}
              stale={verification.stale}
              testId="verification-mark"
            />
          )}
        </KeyValue>
        <KeyValue label="last checked" testId="verification-checked-at-ms">
          {verification.checked_at_ms === null
            ? "never"
            : formatTimestamp(verification.checked_at_ms)}
        </KeyValue>
        <KeyValue label="what DNS answered" testId="verification-detail">
          {verification.detail === null ? (
            "nothing yet"
          ) : (
            <span className="font-mono text-xs break-all">{verification.detail}</span>
          )}
        </KeyValue>
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
    </div>
  );
}
