import { type FormEvent, useState } from "react";

import { type ApiError, setIdentityWitnesses } from "@/api/client";
import type { Identity } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { asApiError } from "@/hooks/useResource";

/**
 * POST /api/identities/:identity_id/witnesses replaces the whole witness set, so
 * adding one endpoint sends the current list plus the new id.
 */
export function WitnessConfigPanel({
  identity,
  onAppended,
}: {
  identity: Identity;
  onAppended: () => void;
}) {
  const [endpoint, setEndpoint] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [headSeq, setHeadSeq] = useState<number | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    setHeadSeq(null);
    try {
      const response = await setIdentityWitnesses(identity.identity_id, {
        witnesses: [...identity.witnesses, endpoint.trim()],
      });
      setHeadSeq(response.head_seq);
      setEndpoint("");
      onAppended();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <div data-testid="witness-config" className="space-y-3">
      {identity.witnesses.length === 0 ? (
        <p data-testid="witness-list-empty" className="text-sm">
          no witnesses configured
        </p>
      ) : (
        <ul data-testid="witness-list" className="space-y-1">
          {identity.witnesses.map((id) => (
            <li key={id} data-testid={`witness-row-${id}`}>
              <Identifier value={id} />
            </li>
          ))}
        </ul>
      )}
      <form onSubmit={submit} className="space-y-2" data-testid="witness-add-form">
        <div className="space-y-1">
          <Label htmlFor="witness-add-endpoint">endpoint id</Label>
          <Input
            id="witness-add-endpoint"
            data-testid="witness-add-endpoint"
            value={endpoint}
            onChange={(event) => setEndpoint(event.target.value)}
            placeholder="52 base32 characters"
          />
        </div>
        <Button type="submit" data-testid="witness-add-submit" disabled={pending}>
          {pending ? "appending" : "Add witness"}
        </Button>
      </form>
      {headSeq !== null && (
        <p data-testid="witness-add-head-seq" className="text-xs">
          head_seq {headSeq}
        </p>
      )}
      {error && <ErrorEnvelopeView error={error} testId="witness-add-error" />}
    </div>
  );
}
