import { type FormEvent, useState } from "react";

import { createIdentity } from "@/api/client";
import type { CreateIdentityResponse, DeclaredKind } from "@/api/types";
import { DeclaredKindNote } from "@/components/DeclaredKind";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import type { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select } from "@/components/ui/select";
import { asApiError } from "@/hooks/useResource";

const KINDS: DeclaredKind[] = ["person", "organization", "agent", "service"];

export function IdentityCreateForm({ onCreated }: { onCreated: () => void }) {
  const [alias, setAlias] = useState("");
  const [declaredKind, setDeclaredKind] = useState<DeclaredKind>("person");
  const [founder, setFounder] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [created, setCreated] = useState<CreateIdentityResponse | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    setCreated(null);
    try {
      const response = await createIdentity({
        alias,
        declared_kind: declaredKind,
        ...(founder.trim() ? { founder: founder.trim() } : {}),
      });
      setCreated(response);
      setAlias("");
      setFounder("");
      onCreated();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground">
        A founder selects an identity root, its absence a raw root
      </p>
      <div>
        <form onSubmit={submit} className="space-y-3" data-testid="identity-create-form">
          <div className="space-y-1">
            <Label htmlFor="identity-create-alias">alias</Label>
            <Input
              id="identity-create-alias"
              data-testid="identity-create-alias"
              value={alias}
              onChange={(event) => setAlias(event.target.value)}
              placeholder="alice"
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="identity-create-declared-kind">declared_kind</Label>
            <Select
              id="identity-create-declared-kind"
              data-testid="identity-create-declared-kind"
              value={declaredKind}
              onChange={(event) => setDeclaredKind(event.target.value as DeclaredKind)}
            >
              {KINDS.map((kind) => (
                <option key={kind} value={kind}>
                  {kind}
                </option>
              ))}
            </Select>
            <DeclaredKindNote testId="identity-create-declared-kind-note" />
          </div>
          <div className="space-y-1">
            <Label htmlFor="identity-create-founder">founder (optional)</Label>
            <Input
              id="identity-create-founder"
              data-testid="identity-create-founder"
              value={founder}
              onChange={(event) => setFounder(event.target.value)}
              placeholder="identity id of the founding principal"
            />
          </div>
          <Button type="submit" data-testid="identity-create-submit" disabled={pending}>
            {pending ? "creating" : "Create"}
          </Button>
        </form>
        {error && (
          <div className="mt-3">
            <ErrorEnvelopeView error={error} testId="identity-create-error" />
          </div>
        )}
        {created && (
          <div className="mt-3 space-y-1 text-xs" data-testid="identity-create-result">
            <p data-testid="identity-create-result-identity-id">
              <Identifier value={created.identity.identity_id} />
            </p>
            <p data-testid="identity-create-result-inception-event">
              <Identifier value={created.inception_event} />
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
