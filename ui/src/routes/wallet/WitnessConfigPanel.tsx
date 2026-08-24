import { type FormEvent, useState } from "react";

import { type ApiError, getIdentity, setIdentityWitnesses } from "@/api/client";
import type { Identity, ResolvedIdentity } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { bareIdentity, IdentityInline, IdentityListScope } from "@/components/identity";
import { InlineField, InlineForm } from "@/components/InlineForm";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { asApiError } from "@/hooks/useResource";

/** What a reader is told when the id they typed is already in the set. */
export const WITNESS_ALREADY_NAMED = "This witness already keeps a copy of this record.";

/**
 * The two refusals a witness id earns, in the words a reader can act on. The
 * node's own message names the id; these name what to do instead.
 */
export const WITNESS_REFUSALS: Record<string, string> = {
  unresolvable_witness:
    "Your wallet found no machine that answers for that identity, so it cannot tell whether the identity exists. Ask whoever runs the witness for a link, which carries a machine to try.",
  endpoint_not_identity:
    "That is the id of a machine, not of an identity. A witness has a Mabel ID of its own, and its machines are listed on its record.",
};

/**
 * Who keeps a copy of this identity's record. A witness is an identity, so this
 * takes a Mabel ID (proposal 006 section 1). The route replaces the whole set,
 * so adding one witness sends the current list plus the new id.
 */
export function WitnessConfigPanel({
  identity,
  names = bareIdentity,
  onAppended,
}: {
  identity: Identity;
  /** Names one witness, for a screen that already holds the witness list. */
  names?: (identityId: string) => ResolvedIdentity;
  onAppended: () => void;
}) {
  const [witness, setWitness] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [duplicate, setDuplicate] = useState(false);
  const [headSeq, setHeadSeq] = useState<number | null>(null);
  const wanted = witness.trim();
  const refused = error === null ? undefined : WITNESS_REFUSALS[error.reason];

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
      setWitness("");
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
        <IdentityListScope identities={identity.witnesses.map(names)}>
          <ul data-testid="witness-list" className="grid gap-2">
            {identity.witnesses.map((id) => (
              <li key={id} className="min-w-0">
                {/* A witness is an identity, so it is named the way every other
                    identity is, and its link opens its own page. */}
                <IdentityInline
                  identity={names(id)}
                  testId={`witness-row-${id}`}
                  to={`/identities/${id}`}
                  full
                />
              </li>
            ))}
          </ul>
        </IdentityListScope>
      )}
      <InlineForm onSubmit={submit} data-testid="witness-add-form">
        <InlineField label="Witness Mabel ID" htmlFor="witness-add-identity">
          <Input
            id="witness-add-identity"
            data-testid="witness-add-identity"
            value={witness}
            onChange={(event) => setWitness(event.target.value)}
            placeholder="paste the witness's Mabel ID"
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
      {refused !== undefined && (
        <p data-testid="witness-add-refused" className="text-sm">
          {refused}
        </p>
      )}
      {error && <ErrorEnvelopeView error={error} testId="witness-add-error" />}
    </div>
  );
}
