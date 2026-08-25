import { useState } from "react";

import { type ApiError, fetchIdentity } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Section } from "@/components/Section";
import { Button } from "@/components/ui/button";
import { asApiError } from "@/hooks/useResource";

/**
 * What using a link does, said before the fetch runs. Handing a link over is
 * one thing; using one is another, and this is the second (proposal 006 section
 * 7).
 */
export const LINK_FETCH_NOTE =
  "This link names the endpoints to ask for this record. Asking them tells those endpoints this home's network address and which identity it is looking for.";

/**
 * Asks for a record this home does not hold in full. Viewing never writes, so
 * nothing runs until the button is pressed (proposal 004). With no machine
 * named the node tries every source it knows; a link's machines are tried
 * first, one request each, in the order the link carried them.
 */
export function FetchButton({
  identityId,
  onFetched,
  machines = [],
  testId = "identity-fetch-button",
}: {
  identityId: string;
  onFetched: () => void;
  /** The machines a link named, dialled first and in order. */
  machines?: string[];
  testId?: string;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);

  async function run() {
    setPending(true);
    setError(null);
    // One machine per request: the route takes one hint. A machine that does
    // not answer is not the end of the attempt, so the next one is tried and
    // only the last failure is reported.
    const sources = machines.length === 0 ? [null] : machines;
    let failure: ApiError | null = null;
    for (const source of sources) {
      try {
        await fetchIdentity(identityId, { from: source });
        setPending(false);
        onFetched();
        return;
      } catch (thrown) {
        failure = asApiError(thrown);
      }
    }
    setError(failure);
    setPending(false);
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
  machines = [],
}: {
  identityId: string;
  onFetched: () => void;
  machines?: string[];
}) {
  return (
    <Section
      testId="identity-fetch"
      title="Fetch this record from a witness"
      description={
        machines.length === 0
          ? "Asks the witnesses your wallet knows, in order, and keeps what they send."
          : "Asks the endpoints the link named, in order, and keeps what they send."
      }
    >
      {machines.length > 0 && (
        <p data-testid="identity-fetch-link-note" className="text-sm">
          {LINK_FETCH_NOTE}
        </p>
      )}
      <FetchButton identityId={identityId} onFetched={onFetched} machines={machines} />
    </Section>
  );
}
