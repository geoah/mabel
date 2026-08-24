import { type FormEvent, useCallback, useState } from "react";

import { addTrust, type ApiError, revokeTrust } from "@/api/client";
import type { Identity } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  factsFromResolved,
  type IdentityCardEntry,
  IdentityCardList,
  nameWithNickname,
  resolvedFrom,
} from "@/components/identity";
import { Identifier } from "@/components/Identifier";
import { InlineField, InlineForm } from "@/components/InlineForm";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { named, type ResolvedNames } from "@/hooks/useResolvedNames";
import { asApiError } from "@/hooks/useResource";

/**
 * Attesting and revoking are one operation pair over one ledger, so they share
 * one pending flag and one "what was appended" line. Which of the two ran last
 * is recorded, so a refusal is reported by the form that caused it and not by
 * its sibling.
 */
export type TrustOperation = "add" | "revoke";

export interface TrustActions {
  /** Resolves true when the append landed, so a form knows whether to clear. */
  add: (subject: string) => Promise<boolean>;
  revoke: (attestationEvent: string) => Promise<boolean>;
  pending: boolean;
  appended: string | null;
  error: ApiError | null;
  /** Which form the pending flag, the error and the appended line belong to. */
  last: TrustOperation | null;
}

export function useTrustActions(issuer: string, onAppended: () => void): TrustActions {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [appended, setAppended] = useState<string | null>(null);
  const [last, setLast] = useState<TrustOperation | null>(null);

  const run = useCallback(
    async (operation: TrustOperation, append: () => Promise<string>) => {
      setPending(true);
      setLast(operation);
      setError(null);
      setAppended(null);
      try {
        setAppended(await append());
        onAppended();
        return true;
      } catch (thrown) {
        setError(asApiError(thrown));
        return false;
      } finally {
        setPending(false);
      }
    },
    [onAppended],
  );

  const add = useCallback(
    (subject: string) =>
      run("add", async () => (await addTrust({ issuer, subject })).event.event_id),
    [issuer, run],
  );

  const revoke = useCallback(
    (attestationEvent: string) =>
      run(
        "revoke",
        async () => (await revokeTrust(attestationEvent, { issuer })).event.event_id,
      ),
    [issuer, run],
  );

  return { add, revoke, pending, appended, error, last };
}

/** The action: saying, once and in public, that you trust one identity. */
export function TrustAddForm({ actions }: { actions: TrustActions }) {
  const [subject, setSubject] = useState("");

  async function submit(event: FormEvent) {
    event.preventDefault();
    // A refused append leaves the subject in the box: retrying is the same
    // action, run again, not the same identity id typed again.
    if (await actions.add(subject.trim())) {
      setSubject("");
    }
  }

  return (
    <div className="space-y-3">
      <InlineForm onSubmit={submit} data-testid="trust-add-form">
        <InlineField label="Who do you trust" htmlFor="trust-add-subject">
          <Input
            id="trust-add-subject"
            data-testid="trust-add-subject"
            value={subject}
            onChange={(event) => setSubject(event.target.value)}
            placeholder="their Mabel ID"
            className="font-mono text-xs"
          />
        </InlineField>
        <Button type="submit" data-testid="trust-add-submit" disabled={actions.pending}>
          {actions.pending && actions.last === "add" ? "saving" : "I trust them"}
        </Button>
      </InlineForm>
      {actions.error && actions.last === "add" && (
        <ErrorEnvelopeView error={actions.error} testId="trust-error" />
      )}
    </div>
  );
}

/**
 * The action: taking back trust you said in public. You name the identity, not
 * the entry: the entry that said it is on the record this page already holds, so
 * the form finds it and takes that one back. An id this identity does not trust
 * right now is refused here, before anything is signed.
 */
export function TrustRevokeForm({
  identity,
  actions,
}: {
  identity: Identity;
  actions: TrustActions;
}) {
  const [subject, setSubject] = useState("");
  const [missing, setMissing] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const wanted = subject.trim();
    setMissing(false);
    const standing = identity.trust.find(
      (record) => !record.revoked && record.subject === wanted,
    );
    if (standing === undefined) {
      setMissing(true);
      return;
    }
    if (await actions.revoke(standing.attestation_event)) {
      setSubject("");
    }
  }

  return (
    <div className="space-y-3">
      <p className="text-sm">
        Taking it back does not erase it. Both the trust and the change stay on the record, so
        anyone reading it sees both.
      </p>
      <InlineForm onSubmit={submit} data-testid="trust-revoke-form">
        <InlineField label="Whose trust do you take back" htmlFor="trust-revoke-subject">
          <Input
            id="trust-revoke-subject"
            data-testid="trust-revoke-subject"
            value={subject}
            onChange={(event) => {
              setMissing(false);
              setSubject(event.target.value);
            }}
            placeholder="their Mabel ID"
            className="font-mono text-xs"
          />
        </InlineField>
        <Button
          type="submit"
          variant="outline"
          data-testid="trust-revoke-submit"
          disabled={actions.pending}
        >
          {actions.pending && actions.last === "revoke" ? "saving" : "Take it back"}
        </Button>
      </InlineForm>
      {missing && (
        <p data-testid="trust-revoke-none" className="text-sm">
          This identity does not trust that id right now, so there is nothing to take back.
        </p>
      )}
      {actions.error && actions.last === "revoke" && (
        <ErrorEnvelopeView error={actions.error} testId="trust-revoke-error" />
      )}
    </div>
  );
}

/**
 * The state: who this identity trusts, one full card each, the same card every
 * other list of identities draws. Trust taken back is not drawn here at all: it
 * stays on the record forever, and the record is where it is read.
 */
export function TrustPanel({
  identity,
  names,
  actions,
}: {
  identity: Identity;
  /** The crawl's name and distance for each subject, keyed by identity id. */
  names: ResolvedNames;
  actions: TrustActions;
}) {
  // The heading names the identity the way its card does, the nickname this
  // device keeps in parentheses after the name it publishes.
  const owner = nameWithNickname(resolvedFrom(identity));
  const entries: IdentityCardEntry[] = identity.trust
    .filter((record) => !record.revoked)
    .map((record) => ({
      facts: factsFromResolved(named(names, record.subject), {
        to: `/identities/${record.subject}`,
      }),
    }));

  return (
    <Card data-testid="trust-panel">
      <CardHeader>
        <CardTitle>{owner === null ? "Who this identity trusts" : `Who ${owner} trusts`}</CardTitle>
        <CardDescription>
          Everyone it has said it trusts and has not taken back.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <IdentityCardList
          entries={entries}
          testId="trust-list"
          empty="This identity has not said it trusts anyone yet."
          emptyTestId="trust-list-empty"
        />
        {actions.appended && (
          <p data-testid="trust-appended-event" className="text-xs">
            Saved as entry <Identifier value={actions.appended} />
          </p>
        )}
      </CardContent>
    </Card>
  );
}
