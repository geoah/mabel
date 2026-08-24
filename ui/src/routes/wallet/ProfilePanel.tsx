import { type FormEvent, useState } from "react";

import { type ApiError, replaceProfile } from "@/api/client";
import type { Identity, ProfileFields, ReplaceProfileResponse } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { asApiError } from "@/hooks/useResource";

function trimmedOrNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

function shown(value: string | null): string {
  return value ?? "none";
}

/**
 * The public name and the public email. Both are replaced together and an empty
 * box clears that one, because the record carries one profile and never a patch.
 * The handle rides along untouched: it has its own action, which also shows the
 * DNS line it needs (proposal 003 section 1, extended with the public email by
 * proposal 005).
 *
 * The confirmation shows the before-and-after diff, the same one
 * `mabel profile replace` prints.
 */
export function ProfilePanel({
  identity,
  onAppended,
}: {
  identity: Identity;
  onAppended: () => void;
}) {
  const current: ProfileFields = {
    display_name: identity.profile?.display_name ?? null,
    hostname: identity.profile?.hostname ?? null,
    email: identity.profile?.email ?? null,
  };
  const [displayName, setDisplayName] = useState(current.display_name ?? "");
  const [email, setEmail] = useState(current.email ?? "");
  const [proposed, setProposed] = useState<ProfileFields | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [replaced, setReplaced] = useState<ReplaceProfileResponse | null>(null);

  function propose(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setReplaced(null);
    setProposed({
      display_name: trimmedOrNull(displayName),
      // The handle this identity publishes is not this form's to change.
      hostname: current.hostname,
      email: trimmedOrNull(email),
    });
  }

  async function confirm() {
    if (proposed === null) {
      return;
    }
    setPending(true);
    setError(null);
    try {
      const response = await replaceProfile(identity.identity_id, proposed);
      setReplaced(response);
      setProposed(null);
      onAppended();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <div data-testid="profile-panel" className="space-y-3">
      <KeyValueTable>
        <KeyValue label="public name now" testId="profile-current-display-name">
          {shown(current.display_name)}
        </KeyValue>
        <KeyValue label="public email now" testId="profile-current-email">
          {shown(current.email)}
        </KeyValue>
      </KeyValueTable>
      <p className="text-xs text-muted-foreground">
        Both are replaced together. Leaving a box empty clears that one, and the handle stays as it
        is.
      </p>
      <form onSubmit={propose} className="space-y-2" data-testid="profile-replace-form">
        <div className="space-y-1">
          <Label htmlFor="profile-display-name">Public name</Label>
          <Input
            id="profile-display-name"
            data-testid="profile-display-name"
            value={displayName}
            onChange={(event) => setDisplayName(event.target.value)}
            placeholder="Alice Ashworth"
          />
        </div>
        <div className="space-y-1">
          <Label htmlFor="profile-email">Public email</Label>
          <Input
            id="profile-email"
            data-testid="profile-email"
            value={email}
            onChange={(event) => setEmail(event.target.value)}
            placeholder="alice@alice.example"
          />
        </div>
        <Button type="submit" data-testid="profile-replace-submit" disabled={pending}>
          Review the change
        </Button>
      </form>
      {proposed && (
        <div className="space-y-2 rounded-md border p-2" data-testid="profile-diff">
          <KeyValueTable>
            <KeyValue label="public name" testId="profile-diff-display-name">
              <span data-testid="profile-diff-display-name-before">
                {shown(current.display_name)}
              </span>{" "}
              becomes{" "}
              <span data-testid="profile-diff-display-name-after">
                {shown(proposed.display_name)}
              </span>
            </KeyValue>
            <KeyValue label="public email" testId="profile-diff-email">
              <span data-testid="profile-diff-email-before">{shown(current.email)}</span> becomes{" "}
              <span data-testid="profile-diff-email-after">{shown(proposed.email)}</span>
            </KeyValue>
          </KeyValueTable>
          <div className="flex gap-2">
            <Button
              size="sm"
              data-testid="profile-replace-confirm"
              disabled={pending}
              onClick={() => void confirm()}
            >
              Confirm
            </Button>
            <Button
              size="sm"
              variant="outline"
              data-testid="profile-replace-cancel"
              disabled={pending}
              onClick={() => setProposed(null)}
            >
              Cancel
            </Button>
          </div>
        </div>
      )}
      {error && <ErrorEnvelopeView error={error} testId="profile-error" />}
      {replaced && (
        <p data-testid="profile-replace-result" className="text-xs">
          Saved at position {replaced.profile.seq}.
        </p>
      )}
    </div>
  );
}
