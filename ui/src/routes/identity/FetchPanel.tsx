import { useState } from "react";

import { type ApiError, fetchIdentity } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { asApiError } from "@/hooks/useResource";

/**
 * The one action a page offers for a ledger this home does not hold: fetch it
 * from a known witness. Viewing never writes, so nothing here runs until the
 * button is pressed (proposal 004).
 */
export function FetchPanel({
  identityId,
  onFetched,
}: {
  identityId: string;
  onFetched: () => void;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  // A fetch that lands makes this page a stored one, and the panel goes with
  // the state it described: the new page is the confirmation.
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
    <Card data-testid="identity-fetch">
      <CardHeader>
        <CardTitle>This home holds no copy of this ledger</CardTitle>
        <CardDescription>
          Everything above is what the crawl read. Fetching asks the known witnesses, in order,
          for the chain itself and stores what they serve.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-2">
        <Button data-testid="identity-fetch-button" disabled={pending} onClick={() => void run()}>
          {pending ? "fetching" : "Fetch from a witness"}
        </Button>
        {error && <ErrorEnvelopeView error={error} testId="identity-fetch-error" />}
      </CardContent>
    </Card>
  );
}
