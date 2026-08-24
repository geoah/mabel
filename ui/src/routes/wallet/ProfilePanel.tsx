import { type FormEvent, useState } from "react";

import { type ApiError, replaceProfile } from "@/api/client";
import type { Identity, ProfileFields, ReplaceProfileResponse } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { asApiError } from "@/hooks/useResource";
import { HOSTNAME_CONSENT_KEY, useConsent } from "@/lib/preferences";

/**
 * What publishing a hostname makes public, stated before the first one and
 * remembered per node home (proposal 003, Consequences).
 */
const HOSTNAME_CONSENT_SENTENCES = [
  "Every name and website you set here stays readable forever by anyone who knows this identity's id.",
  "Changing it later hides nothing: the old ones stay on the record, and copies are already out there.",
];

function trimmedOrNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

function shown(value: string | null): string {
  return value ?? "none";
}

/**
 * Profile replacement, never a patch: both fields are always sent and an empty
 * box clears that name. The confirmation shows the before-and-after diff, the
 * same one `mabel profile replace` prints (proposal 003 section 1).
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
  };
  const [displayName, setDisplayName] = useState(current.display_name ?? "");
  const [hostname, setHostname] = useState(current.hostname ?? "");
  const [proposed, setProposed] = useState<ProfileFields | null>(null);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [replaced, setReplaced] = useState<ReplaceProfileResponse | null>(null);
  const [consented, giveConsent] = useConsent(HOSTNAME_CONSENT_KEY);

  // Consent is asked for once, and only when a replacement would publish a
  // hostname this node home has never published before.
  const publishing =
    proposed !== null && proposed.hostname !== null && proposed.hostname !== current.hostname;
  const asking = publishing && !consented;

  function propose(event: FormEvent) {
    event.preventDefault();
    setError(null);
    setReplaced(null);
    setProposed({
      display_name: trimmedOrNull(displayName),
      hostname: trimmedOrNull(hostname),
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
      if (publishing) {
        giveConsent();
      }
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
        <KeyValue label="website now" testId="profile-current-hostname">
          <span className="font-mono text-xs">{shown(current.hostname)}</span>
        </KeyValue>
      </KeyValueTable>
      <p className="text-xs text-muted-foreground">
        Both are replaced together. Leaving a box empty clears that one.
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
          <Label htmlFor="profile-hostname">Website</Label>
          <Input
            id="profile-hostname"
            data-testid="profile-hostname"
            value={hostname}
            onChange={(event) => setHostname(event.target.value)}
            placeholder="alice.example"
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
            <KeyValue label="website" testId="profile-diff-hostname">
              <span data-testid="profile-diff-hostname-before" className="font-mono text-xs">
                {shown(current.hostname)}
              </span>{" "}
              becomes{" "}
              <span data-testid="profile-diff-hostname-after" className="font-mono text-xs">
                {shown(proposed.hostname)}
              </span>
            </KeyValue>
          </KeyValueTable>
          {asking && (
            <div data-testid="profile-hostname-consent" className="space-y-1">
              {HOSTNAME_CONSENT_SENTENCES.map((sentence) => (
                <p key={sentence} className="text-xs">
                  {sentence}
                </p>
              ))}
            </div>
          )}
          <div className="flex gap-2">
            <Button
              size="sm"
              data-testid="profile-replace-confirm"
              disabled={pending}
              onClick={() => void confirm()}
            >
              {asking ? "Publish and replace" : "Confirm"}
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
