import { type FormEvent, useState } from "react";

import { type ApiError, getNode, setIdentityEndpoints } from "@/api/client";
import type { Identity } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { InlineField, InlineForm } from "@/components/InlineForm";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useResource } from "@/hooks/useResource";
import { asApiError } from "@/hooks/useResource";
import { ENDPOINTS_CONSENT_KEY, useConsent } from "@/lib/preferences";

/**
 * The three facts publishing an endpoint puts in front of a person, once per home
 * (proposal 006 section 8). They are what an advertisement costs, not a warning
 * about it.
 */
export const ENDPOINTS_CONSENT_SENTENCES = [
  "The endpoint's id stays readable forever by anyone who can name this identity.",
  "Anyone who reads it can dial that endpoint directly, which shows the endpoint's address to them and to the relay that connects them.",
  "Once this home answers at a published address, anyone who dials it can list the identities it signs for and, if it keeps records for other people, the records it keeps.",
];

/** What a reader is told when the endpoint they typed is already published. */
export const MACHINE_ALREADY_PUBLISHED = "This endpoint is already on this identity's record.";

/**
 * The endpoints this identity's own record says answer for it. The route
 * replaces the list whole, so publishing one sends the list this identity
 * already carries plus the new id.
 */
export function EndpointsPanel({
  identity,
  onAppended,
}: {
  identity: Identity;
  onAppended: () => void;
}) {
  const node = useResource(getNode, []);
  const [machine, setMachine] = useState("");
  const [pending, setPending] = useState(false);
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [duplicate, setDuplicate] = useState(false);
  const [headSeq, setHeadSeq] = useState<number | null>(null);
  const [consented, giveConsent] = useConsent(ENDPOINTS_CONSENT_KEY);
  const wanted = machine.trim();
  const published = identity.endpoints;

  async function publish() {
    setPending(true);
    setError(null);
    setHeadSeq(null);
    try {
      const response = await setIdentityEndpoints(identity.identity_id, {
        endpoints: [...published, wanted],
      });
      giveConsent();
      setAsking(false);
      setHeadSeq(response.head_seq);
      setMachine("");
      onAppended();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  function submit(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setHeadSeq(null);
    setDuplicate(false);
    if (published.includes(wanted)) {
      setDuplicate(true);
      return;
    }
    // Consent is asked for once per home, and nothing is signed before it.
    if (!consented) {
      setAsking(true);
      return;
    }
    void publish();
  }

  return (
    <div data-testid="endpoints-panel" className="space-y-3">
      {published.length === 0 ? (
        <p data-testid="endpoints-empty" className="text-sm">
          This identity&apos;s record names no endpoint yet.
        </p>
      ) : (
        <ul data-testid="endpoints-list" className="grid gap-2">
          {published.map((endpointId) => (
            <li key={endpointId} className="min-w-0">
              <Identifier
                value={endpointId}
                full
                copyLabel="Copy endpoint ID"
                className="text-xs"
              />
            </li>
          ))}
        </ul>
      )}
      <InlineForm onSubmit={submit} data-testid="endpoints-form">
        <InlineField label="Endpoint ID" htmlFor="endpoints-input">
          <Input
            id="endpoints-input"
            data-testid="endpoints-input"
            value={machine}
            onChange={(event) => setMachine(event.target.value)}
            placeholder="paste the Iroh ID of an endpoint"
            className="font-mono text-xs"
          />
        </InlineField>
        <Button type="submit" data-testid="endpoints-submit" disabled={pending || wanted === ""}>
          {pending ? "publishing" : "Publish"}
        </Button>
      </InlineForm>
      {node.data && (
        <Button
          type="button"
          size="sm"
          variant="outline"
          data-testid="endpoints-use-this-node"
          onClick={() => setMachine(node.data!.endpoint_id)}
        >
          Use this node
        </Button>
      )}
      {asking && (
        <div data-testid="endpoints-consent" className="space-y-2 rounded-md border p-2">
          {ENDPOINTS_CONSENT_SENTENCES.map((sentence) => (
            <p key={sentence} className="text-xs">
              {sentence}
            </p>
          ))}
          <div className="flex gap-2">
            <Button
              size="sm"
              data-testid="endpoints-consent-confirm"
              disabled={pending}
              onClick={() => void publish()}
            >
              Publish the endpoint
            </Button>
            <Button
              size="sm"
              variant="outline"
              data-testid="endpoints-consent-cancel"
              disabled={pending}
              onClick={() => setAsking(false)}
            >
              Cancel
            </Button>
          </div>
        </div>
      )}
      {duplicate && (
        <p data-testid="endpoints-duplicate" className="text-sm">
          {MACHINE_ALREADY_PUBLISHED}
        </p>
      )}
      {headSeq !== null && (
        <p data-testid="endpoints-head-seq" className="text-xs">
          Saved at position {headSeq}.
        </p>
      )}
      {error && <ErrorEnvelopeView error={error} testId="endpoints-error" />}
    </div>
  );
}
