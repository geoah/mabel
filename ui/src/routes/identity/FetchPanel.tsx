import { useState } from "react";

import { type ApiError, fetchIdentity } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
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
    <Card data-testid="identity-fetch">
      <CardHeader>
        <CardTitle>Fetch this record from a witness</CardTitle>
        <CardDescription>
          Everything above is what your wallet found by following who trusts whom. Fetching asks
          the witnesses it knows, in order, for the record itself and keeps what they send.
        </CardDescription>
      </CardHeader>
      <CardContent>
        <FetchButton identityId={identityId} onFetched={onFetched} />
      </CardContent>
    </Card>
  );
}
