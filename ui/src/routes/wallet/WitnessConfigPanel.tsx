import { type FormEvent, useState } from "react";

import { type ApiError, getIdentity, setIdentityWitnesses } from "@/api/client";
import type { Identity } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { InlineField, InlineForm } from "@/components/InlineForm";
import { WitnessCard } from "@/components/WitnessCard";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { asApiError } from "@/hooks/useResource";

/** What a reader is told when the id they typed is already in the set. */
export const WITNESS_ALREADY_NAMED = "This witness already keeps a copy of this record.";

/**
 * Who keeps a copy of this identity's record. The route replaces the whole set,
 * so adding one witness sends the current list plus the new id.
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
  const [duplicate, setDuplicate] = useState(false);
  const [headSeq, setHeadSeq] = useState<number | null>(null);
  const wanted = endpoint.trim();

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    setDuplicate(false);
    setHeadSeq(null);
    try {
      // The route replaces the whole set, so the set this send is built on has
      // to be the one the node holds now: the identity document this panel was
      // rendered from may be seconds old, and a witness added meanwhile would be
      // dropped by a stale base. Reading it here narrows that window to this
      // request pair rather than to however long the panel has been open.
      const current = (await getIdentity(identity.identity_id)).identity.witnesses;
      if (current.includes(wanted)) {
        setDuplicate(true);
        return;
      }
      const response = await setIdentityWitnesses(identity.identity_id, {
        witnesses: [...current, wanted],
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
          No witness keeps a copy of this record yet.
        </p>
      ) : (
        <ul data-testid="witness-list" className="grid gap-2">
          {identity.witnesses.map((id) => (
            <li key={id} className="min-w-0">
              {/* The same card the list of witnesses draws, opening the same
                  page: a witness is one thing, wherever it is named. */}
              <WitnessCard endpointId={id} testIdPrefix="witness-row" />
            </li>
          ))}
        </ul>
      )}
      <InlineForm onSubmit={submit} data-testid="witness-add-form">
        <InlineField label="Witness Iroh ID" htmlFor="witness-add-endpoint">
          <Input
            id="witness-add-endpoint"
            data-testid="witness-add-endpoint"
            value={endpoint}
            onChange={(event) => setEndpoint(event.target.value)}
            placeholder="52 base32 characters"
            className="font-mono text-xs"
          />
        </InlineField>
        <Button
          type="submit"
          data-testid="witness-add-submit"
          disabled={pending || wanted === ""}
        >
          {pending ? "adding" : "Add witness"}
        </Button>
      </InlineForm>
      {duplicate && (
        <p data-testid="witness-add-duplicate" className="text-sm">
          {WITNESS_ALREADY_NAMED}
        </p>
      )}
      {headSeq !== null && (
        <p data-testid="witness-add-head-seq" className="text-xs">
          Saved at position {headSeq}.
        </p>
      )}
      {error && <ErrorEnvelopeView error={error} testId="witness-add-error" />}
    </div>
  );
}
