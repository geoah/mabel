import { type FormEvent, useCallback, useState } from "react";

import { addTrust, type ApiError, revokeTrust } from "@/api/client";
import type { Identity, ResolvedIdentity as ResolvedIdentityDocument, TrustRecord } from "@/api/types";
import { DeveloperOnly } from "@/components/DeveloperMode";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import {
  ResolvedIdentity,
  ResolvedIdentityScope,
  resolveName,
  resolvedFrom,
} from "@/components/ResolvedIdentity";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { asApiError } from "@/hooks/useResource";

/**
 * Attesting and revoking are one operation pair over one ledger, so they share
 * one pending flag, one error and one "what was appended" line. The form lives
 * in the actions section and the revoke buttons live on the rows of the trust
 * list, which is why the state is lifted here rather than held in either.
 */
export interface TrustActions {
  /** Resolves true when the append landed, so a form knows whether to clear. */
  add: (subject: string) => Promise<boolean>;
  revoke: (attestationEvent: string) => Promise<boolean>;
  pending: boolean;
  appended: string | null;
  error: ApiError | null;
}

export function useTrustActions(issuer: string, onAppended: () => void): TrustActions {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [appended, setAppended] = useState<string | null>(null);

  const run = useCallback(
    async (append: () => Promise<string>) => {
      setPending(true);
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
      run(async () => (await addTrust({ issuer, subject })).event.event_id),
    [issuer, run],
  );

  const revoke = useCallback(
    (attestationEvent: string) =>
      run(async () => (await revokeTrust(attestationEvent, { issuer })).event.event_id),
    [issuer, run],
  );

  return { add, revoke, pending, appended, error };
}

/** The action: one attestation naming one subject. */
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
      <form onSubmit={submit} className="space-y-2" data-testid="trust-add-form">
        <div className="space-y-1">
          <Label htmlFor="trust-add-subject">subject</Label>
          <Input
            id="trust-add-subject"
            data-testid="trust-add-subject"
            value={subject}
            onChange={(event) => setSubject(event.target.value)}
            placeholder="identity id of the subject"
          />
        </div>
        <Button type="submit" data-testid="trust-add-submit" disabled={actions.pending}>
          {actions.pending ? "appending" : "Attest trust"}
        </Button>
      </form>
      {actions.error && <ErrorEnvelopeView error={actions.error} testId="trust-error" />}
    </div>
  );
}

function TrustRow({
  record,
  resolved,
  actions,
}: {
  record: TrustRecord;
  resolved: ResolvedIdentityDocument;
  actions: TrustActions;
}) {
  return (
    <li
      data-testid={`trust-row-${record.attestation_event}`}
      className="flex flex-wrap items-center gap-x-3 gap-y-1 py-2"
    >
      <ResolvedIdentity
        identity={resolved}
        testId={`trust-subject-${record.attestation_event}`}
        to={`/wallet/lookup/${record.subject}`}
      />
      <span
        data-testid={`trust-state-${record.attestation_event}`}
        className="text-xs text-muted-foreground"
      >
        {record.revoked ? `revoked at seq ${record.revocation_seq}` : "unrevoked"}
      </span>
      <Button
        variant="outline"
        size="sm"
        className="ml-auto"
        disabled={actions.pending || record.revoked}
        onClick={() => void actions.revoke(record.attestation_event)}
        data-testid={`trust-revoke-${record.attestation_event}`}
      >
        Revoke
      </Button>
      <DeveloperOnly>
        <span
          data-testid={`trust-attestation-seq-${record.attestation_event}`}
          className="w-full text-xs text-muted-foreground"
        >
          attested at seq {record.attestation_seq}, event{" "}
          <Identifier value={record.attestation_event} />
        </span>
      </DeveloperOnly>
    </li>
  );
}

/**
 * The state: who this identity trusts, by resolved name, each row linking to
 * the lookup that answers how the wallet knows them. Revoked attestations stay
 * in the chain forever, so they stay on the screen, folded away.
 */
export function TrustPanel({
  identity,
  names,
  actions,
}: {
  identity: Identity;
  /** The crawl's name for each subject, keyed by identity id. */
  names: Map<string, ResolvedIdentityDocument>;
  actions: TrustActions;
}) {
  const owner = resolveName(resolvedFrom(identity)).name;
  const resolved = (subject: string): ResolvedIdentityDocument =>
    names.get(subject) ?? {
      identity_id: subject,
      display_name: null,
      alias: null,
      hostname: null,
      verification_status: "unclaimed",
      provenance: "none",
    };
  const unrevoked = identity.trust.filter((record) => !record.revoked);
  const revoked = identity.trust.filter((record) => record.revoked);

  return (
    <Card data-testid="trust-panel">
      <CardHeader>
        <CardTitle>{owner === null ? "Trusted" : `Who ${owner} trusts`}</CardTitle>
        <CardDescription>
          One unrevoked attestation per subject, each signed into this ledger
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <ResolvedIdentityScope identities={identity.trust.map((record) => resolved(record.subject))}>
          {unrevoked.length === 0 ? (
            <p data-testid="trust-list-empty" className="text-sm">
              this identity trusts nobody yet
            </p>
          ) : (
            <ul data-testid="trust-list" className="divide-y">
              {unrevoked.map((record) => (
                <TrustRow
                  key={record.attestation_event}
                  record={record}
                  resolved={resolved(record.subject)}
                  actions={actions}
                />
              ))}
            </ul>
          )}
          {revoked.length > 0 && (
            <details data-testid="trust-revoked" className="rounded-md border">
              <summary
                data-testid="trust-revoked-summary"
                className="flex min-h-11 cursor-pointer list-none items-center px-3 text-xs text-muted-foreground marker:content-none hover:bg-accent"
              >
                {revoked.length} revoked {revoked.length === 1 ? "attestation" : "attestations"},
                still in the chain
              </summary>
              <ul className="divide-y border-t px-3">
                {revoked.map((record) => (
                  <TrustRow
                    key={record.attestation_event}
                    record={record}
                    resolved={resolved(record.subject)}
                    actions={actions}
                  />
                ))}
              </ul>
            </details>
          )}
        </ResolvedIdentityScope>
        {actions.appended && (
          <p data-testid="trust-appended-event" className="text-xs">
            appended <Identifier value={actions.appended} />
          </p>
        )}
      </CardContent>
    </Card>
  );
}
