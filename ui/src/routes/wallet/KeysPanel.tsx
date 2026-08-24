import { useState } from "react";

import { ApiError, getIdentityKeys } from "@/api/client";
import type { IdentityKeysResponse } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { useResource } from "@/hooks/useResource";

/**
 * The file a person walks away with. It is plain text on purpose: it has to be
 * readable in a password manager, a printout or a note app, none of which parse
 * JSON.
 */
function keyFile(keys: IdentityKeysResponse): string {
  return [
    "mabel identity keys",
    "",
    `identity: ${keys.identity_id}`,
    "",
    "The key that signs today. Whoever holds it controls this identity.",
    `active secret key: ${keys.active_secret_key}`,
    "",
    "The key you will need if you ever replace the one above.",
    `reserve secret key: ${keys.reserve_secret_key}`,
    "",
    "Public, and safe to share:",
    `active key: ${keys.active_key}`,
    `reserve commitment: ${keys.reserve_commit}`,
    "",
    "Anyone holding the two secret keys controls this identity. Losing both",
    "loses the identity: nobody can reissue them.",
    "",
  ].join("\n");
}

/** One secret, its one plain sentence, and a way to get it out of the browser. */
function SecretKey({
  label,
  sentence,
  value,
  testId,
}: {
  label: string;
  sentence: string;
  value: string;
  testId: string;
}) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard?.writeText(value);
    } catch {
      // No clipboard and no permission: the box beside the button still holds
      // the whole value, so nothing is lost by reporting nothing.
      return;
    }
    setCopied(true);
  }

  return (
    <div className="space-y-1">
      <Label htmlFor={testId}>{label}</Label>
      <p className="text-xs text-muted-foreground">{sentence}</p>
      <textarea
        id={testId}
        data-testid={testId}
        readOnly
        value={value}
        rows={2}
        className="w-full rounded-md border bg-muted px-2 py-1 font-mono text-xs break-all"
      />
      <Button
        variant="outline"
        size="sm"
        data-testid={`${testId}-copy`}
        onBlur={() => setCopied(false)}
        onClick={() => void copy()}
      >
        {copied ? "Copied" : "Copy"}
      </Button>
    </div>
  );
}

/**
 * The two secret keys of one identity, offered for the person to save (decision
 * 017). The node holds a copy either way, so this is not the only copy in
 * existence, and the warning says exactly that rather than implying the browser
 * is the last chance.
 *
 * An identity that holds no key of its own is not an error to a reader: its
 * controllers sign for it, so the 409 renders as that sentence.
 */
export function KeysPanel({ identityId }: { identityId: string }) {
  const keys = useResource(() => getIdentityKeys(identityId), [identityId]);
  const keyless = keys.error instanceof ApiError && keys.error.reason === "no_keys_held";

  return (
    <div data-testid="identity-keys" className="space-y-3">
      {keys.loading && <p data-testid="identity-keys-loading">loading</p>}
      {keyless && (
        <p data-testid="identity-keys-none" className="text-sm">
          This identity holds no key of its own. Its controllers sign for it, and their keys are
          saved with their own identities.
        </p>
      )}
      {keys.error && !keyless && (
        <ErrorEnvelopeView error={keys.error} testId="identity-keys-error" />
      )}
      {keys.data && (
        <>
          <SecretKey
            label="The key that signs today"
            sentence="Every entry this identity adds to its record is signed with this key."
            value={keys.data.active_secret_key}
            testId="identity-keys-active"
          />
          <SecretKey
            label="The key you will need if you ever replace it"
            sentence="Kept aside, unused, for the day the key above has to be swapped out."
            value={keys.data.reserve_secret_key}
            testId="identity-keys-reserve"
          />
          <a
            href={`data:text/plain;charset=utf-8,${encodeURIComponent(keyFile(keys.data))}`}
            download={`mabel-keys-${identityId.slice(0, 8)}.txt`}
            data-testid="identity-keys-download"
            className="inline-flex min-h-9 items-center text-sm underline"
          >
            Download both keys as a text file
          </a>
          <p data-testid="identity-keys-warning" className="text-sm">
            Anyone who has these two keys controls this identity, and losing both loses it: nobody
            can issue them again. This wallet keeps its own copy on this computer, so saving them
            elsewhere protects you against losing the computer, not against someone reading it.
          </p>
        </>
      )}
    </div>
  );
}
