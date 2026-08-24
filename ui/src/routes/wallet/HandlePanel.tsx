import { type FormEvent, useState } from "react";

import { type ApiError, replaceProfile } from "@/api/client";
import type { Identity, ReplaceProfileResponse } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { InlineField, InlineForm } from "@/components/InlineForm";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { asApiError } from "@/hooks/useResource";
import { HOSTNAME_CONSENT_KEY, useConsent } from "@/lib/preferences";

import { VerificationPanel } from "./VerificationPanel";

/**
 * What publishing a handle makes public, stated before the first one and
 * remembered per node home (proposal 003, Consequences).
 */
const HANDLE_CONSENT_SENTENCES = [
  "Every name, email and handle you set here stays readable forever by anyone who knows this identity's id.",
  "Changing it later hides nothing: the old ones stay on the record, and copies are already out there.",
];

/** The DNS line that makes a handle point back at this identity. */
export function txtRecord(handle: string, identityId: string): string {
  return `_mabel.${handle}. IN TXT "mabel=${identityId}"`;
}

/**
 * The handle: the name someone can type instead of a 52-character id. Setting
 * one replaces the profile, which is why the public name and the email travel
 * with it untouched. A handle only works once its DNS records name this
 * identity back, so the line to add is on the screen, and so is the check.
 */
export function HandlePanel({
  identity,
  onAppended,
}: {
  identity: Identity;
  onAppended: () => void;
}) {
  const current = identity.profile?.hostname ?? null;
  const [handle, setHandle] = useState(current ?? "");
  const [pending, setPending] = useState(false);
  const [asking, setAsking] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [replaced, setReplaced] = useState<ReplaceProfileResponse | null>(null);
  const [consented, giveConsent] = useConsent(HOSTNAME_CONSENT_KEY);
  const wanted = handle.trim();
  // The record to add is about the handle you are setting, or the one this
  // identity already publishes while the box is untouched.
  const shown = wanted === "" ? current : wanted;

  async function replace() {
    setPending(true);
    setError(null);
    setReplaced(null);
    try {
      const response = await replaceProfile(identity.identity_id, {
        display_name: identity.profile?.display_name ?? null,
        email: identity.profile?.email ?? null,
        hostname: wanted === "" ? null : wanted,
      });
      if (wanted !== "") {
        giveConsent();
      }
      setAsking(false);
      setReplaced(response);
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
    setReplaced(null);
    // Consent is asked for once, and only for a handle this node home has never
    // published before.
    if (wanted !== "" && wanted !== current && !consented) {
      setAsking(true);
      return;
    }
    void replace();
  }

  return (
    <div data-testid="handle-panel" className="space-y-3">
      <KeyValueTable>
        <KeyValue label="handle now" testId="handle-current">
          {current === null ? (
            "none"
          ) : (
            <span className="font-mono text-xs">{current}</span>
          )}
        </KeyValue>
      </KeyValueTable>
      <InlineForm onSubmit={submit} data-testid="handle-form">
        <InlineField label="Handle" htmlFor="handle-input">
          <Input
            id="handle-input"
            data-testid="handle-input"
            value={handle}
            onChange={(event) => setHandle(event.target.value)}
            placeholder="alice.example"
            className="font-mono text-xs"
          />
        </InlineField>
        <Button type="submit" data-testid="handle-submit" disabled={pending}>
          {pending ? "saving" : "Save"}
        </Button>
      </InlineForm>
      <p className="text-xs text-muted-foreground">
        The public name and the email stay as they are. An empty box takes the handle away.
      </p>
      {asking && (
        <div data-testid="handle-consent" className="space-y-2 rounded-md border p-2">
          {HANDLE_CONSENT_SENTENCES.map((sentence) => (
            <p key={sentence} className="text-xs">
              {sentence}
            </p>
          ))}
          <div className="flex gap-2">
            <Button
              size="sm"
              data-testid="handle-consent-confirm"
              disabled={pending}
              onClick={() => void replace()}
            >
              Publish the handle
            </Button>
            <Button
              size="sm"
              variant="outline"
              data-testid="handle-consent-cancel"
              disabled={pending}
              onClick={() => setAsking(false)}
            >
              Cancel
            </Button>
          </div>
        </div>
      )}
      {error && <ErrorEnvelopeView error={error} testId="handle-error" />}
      {replaced && (
        <p data-testid="handle-result" className="text-xs">
          Saved at position {replaced.profile.seq}.
        </p>
      )}
      <div className="space-y-1 border-t pt-3">
        <p className="text-sm">
          {shown === null
            ? "Set a handle to see the line your DNS records need."
            : "Add this line to the DNS records of your handle, then check it here."}
        </p>
        {shown !== null && (
          <Identifier
            value={txtRecord(shown, identity.identity_id)}
            full
            className="text-xs"
          />
        )}
      </div>
      <VerificationPanel identity={identity} onChecked={onAppended} />
    </div>
  );
}
