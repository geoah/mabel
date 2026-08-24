import { type FormEvent, useState } from "react";

import { createIdentity } from "@/api/client";
import type { CreateIdentityResponse, DeclaredKind } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import type { ApiError } from "@/api/client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select } from "@/components/ui/select";
import { asApiError } from "@/hooks/useResource";

import { KeysPanel } from "./KeysPanel";

const KINDS: DeclaredKind[] = ["person", "organization", "agent", "service"];

function trimmedOrUndefined(value: string): string | undefined {
  const trimmed = value.trim();
  return trimmed === "" ? undefined : trimmed;
}

/**
 * Creating an identity, in the order proposal 005 fixes: the private nickname
 * this device keeps for itself, then the two facts the new identity publishes
 * about itself from birth, then what kind of thing it says it is and who signs
 * for it.
 *
 * A public name or email given here becomes one entry on the new record, right
 * after the one that created it. Leaving both empty publishes nothing.
 */
export function IdentityCreateForm({ onCreated }: { onCreated: () => void }) {
  const [alias, setAlias] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [email, setEmail] = useState("");
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
    const publicName = trimmedOrUndefined(displayName);
    const publicEmail = trimmedOrUndefined(email);
    try {
      const response = await createIdentity({
        alias,
        declared_kind: declaredKind,
        ...(founder.trim() ? { founder: founder.trim() } : {}),
        ...(publicName ? { display_name: publicName } : {}),
        ...(publicEmail ? { email: publicEmail } : {}),
      });
      setCreated(response);
      setAlias("");
      setDisplayName("");
      setEmail("");
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
      <div>
        <form onSubmit={submit} className="space-y-3" data-testid="identity-create-form">
          <div className="space-y-1">
            <Label htmlFor="identity-create-alias">
              Private nickname (only this device sees it)
            </Label>
            <Input
              id="identity-create-alias"
              data-testid="identity-create-alias"
              value={alias}
              onChange={(event) => setAlias(event.target.value)}
              placeholder="alice"
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="identity-create-display-name">Public name (optional)</Label>
            <Input
              id="identity-create-display-name"
              data-testid="identity-create-display-name"
              value={displayName}
              onChange={(event) => setDisplayName(event.target.value)}
              placeholder="Alice Ashworth"
            />
          </div>
          <div className="space-y-1">
            <Label htmlFor="identity-create-email">Public email (optional)</Label>
            <Input
              id="identity-create-email"
              data-testid="identity-create-email"
              value={email}
              onChange={(event) => setEmail(event.target.value)}
              placeholder="alice@alice.example"
            />
            <p className="text-xs text-muted-foreground">
              Anyone who knows this identity&apos;s id can read the public name and email, and can
              still read them after you change them.
            </p>
          </div>
          <div className="space-y-1">
            <Label htmlFor="identity-create-declared-kind">What kind of thing this is</Label>
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
          </div>
          <div className="space-y-1">
            <Label htmlFor="identity-create-founder">Founder (optional)</Label>
            <Input
              id="identity-create-founder"
              data-testid="identity-create-founder"
              value={founder}
              onChange={(event) => setFounder(event.target.value)}
              placeholder="Mabel ID of whoever will sign for it"
            />
            <p className="text-xs text-muted-foreground">
              Leave this empty and the new identity gets a key of its own. Name a founder and that
              identity signs for this one instead, which is how an organization works.
            </p>
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
          <div className="mt-3 space-y-3" data-testid="identity-create-result">
            <KeyValueTable>
              <KeyValue label="Mabel ID" testId="identity-create-result-identity-id">
                <Identifier value={created.identity.identity_id} />
              </KeyValue>
              <KeyValue label="first entry" testId="identity-create-result-inception-event">
                <Identifier value={created.inception_event} />
              </KeyValue>
              {created.identity.profile !== null && (
                <KeyValue label="published" testId="identity-create-result-profile">
                  <span data-testid="identity-create-result-display-name">
                    {created.identity.profile.display_name ?? "no name"}
                  </span>
                  {", "}
                  <span data-testid="identity-create-result-email">
                    {created.identity.profile.email ?? "no email"}
                  </span>
                </KeyValue>
              )}
            </KeyValueTable>
            {/* Creating an identity offers its keys to save, on the spot. */}
            <div className="space-y-2 rounded-md border p-3">
              <p className="text-sm font-medium">Save your keys</p>
              <KeysPanel identityId={created.identity.identity_id} />
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
