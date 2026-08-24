import { useState } from "react";

import { type ApiError, fetchIdentity } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Section } from "@/components/Section";
import { Button } from "@/components/ui/button";
import { asApiError } from "@/hooks/useResource";

/**
 * Asks the known witnesses, in order, for a record this home does not hold in
 * full. Viewing never writes, so nothing runs until the button is pressed
 * (proposal 004). The record panel borrows this button when it holds a summary
 * without the entries behind it.
 */
export function FetchButton({
  identityId,
  onFetched,
  testId = "identity-fetch-button",
}: {
  identityId: string;
  onFetched: () => void;
  testId?: string;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  async function run() {
    setPending(true);
    setError(null);
    try {
      await fetchIdentity(identityId, { from: null });
      onFetched();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <div className="space-y-2">
      <Button data-testid={testId} disabled={pending} onClick={() => void run()}>
        {pending ? "fetching" : "Fetch from a witness"}
      </Button>
      {error && <ErrorEnvelopeView error={error} testId={`${testId.replace(/-button$/, "")}-error`} />}
    </div>
  );
}

/**
 * The one action a page offers for a record this home does not hold at all. A
 * fetch that lands makes this page a stored one, and the panel goes with the
 * state it described: the new page is the confirmation.
 */
export function FetchPanel({
  identityId,
  onFetched,
}: {
  identityId: string;
  onFetched: () => void;
}) {
  return (
    <Section
      testId="identity-fetch"
      title="Fetch this record from a witness"
      description="Asks the witnesses your wallet knows, in order, and keeps what they send."
    >
      <FetchButton identityId={identityId} onFetched={onFetched} />
    </Section>
  );
}
