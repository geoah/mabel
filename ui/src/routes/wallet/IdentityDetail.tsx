import { useCallback, useState } from "react";
import { Link, useParams } from "react-router";

import { getIdentity } from "@/api/client";
import { DeclaredKindNote, DeclaredKindValue } from "@/components/DeclaredKind";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Field, FieldGrid } from "@/components/Field";
import { Identifier } from "@/components/Identifier";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

import { LedgerPanel } from "./LedgerPanel";
import { PrincipalsPanel } from "./PrincipalsPanel";
import { SyncPushPanel } from "./SyncPushPanel";
import { TrustPanel } from "./TrustPanel";
import { WitnessConfigPanel } from "./WitnessConfigPanel";

export function IdentityDetail() {
  const { identityId = "" } = useParams();
  const [version, setVersion] = useState(0);
  const identity = useResource(() => getIdentity(identityId), [identityId, version]);
  const refresh = useCallback(() => setVersion((value) => value + 1), []);

  return (
    <div className="space-y-4">
      <Link
        to="/wallet"
        className="inline-flex min-h-10 items-center text-sm underline"
        data-testid="identity-back"
      >
        Identities
      </Link>
      {identity.loading && <p data-testid="identity-detail-loading">loading</p>}
      {identity.error && (
        <ErrorEnvelopeView error={identity.error} testId="identity-detail-error" />
      )}
      {identity.data && (
        <div className="grid gap-4 lg:grid-cols-2">
          <Card data-testid="identity-detail">
            <CardHeader>
              <CardTitle className="text-base">{identity.data.identity.alias}</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              <FieldGrid>
                <Field label="identity_id" testId="identity-detail-identity-id">
                  <Identifier value={identity.data.identity.identity_id} />
                </Field>
                <Field label="declared_kind" testId="identity-detail-declared-kind-row">
                  <DeclaredKindValue
                    kind={identity.data.identity.declared_kind}
                    testId="identity-detail-declared-kind"
                  />
                </Field>
                <Field label="alias" testId="identity-detail-alias">
                  {identity.data.identity.alias}
                </Field>
                <Field label="created_at_ms" testId="identity-detail-created-at-ms">
                  {identity.data.identity.created_at_ms}
                </Field>
                <Field label="head_seq" testId="identity-detail-head-seq">
                  {identity.data.identity.head_seq}
                </Field>
                <Field label="head_event" testId="identity-detail-head-event">
                  <Identifier value={identity.data.identity.head_event} />
                </Field>
                <Field label="event_count" testId="identity-detail-event-count">
                  {identity.data.identity.event_count}
                </Field>
                <Field label="active_key" testId="identity-detail-active-key">
                  <Identifier value={identity.data.identity.active_key} />
                </Field>
                <Field label="reserve_commit" testId="identity-detail-reserve-commit">
                  <Identifier value={identity.data.identity.reserve_commit} />
                </Field>
              </FieldGrid>
              <DeclaredKindNote testId="identity-detail-declared-kind-note" />
            </CardContent>
          </Card>
          <PrincipalsPanel identity={identity.data.identity} />
          <WitnessConfigPanel identity={identity.data.identity} onAppended={refresh} />
          <TrustPanel identity={identity.data.identity} onAppended={refresh} />
          <SyncPushPanel identityId={identity.data.identity.identity_id} />
          <div className="lg:col-span-2">
            <LedgerPanel identityId={identity.data.identity.identity_id} version={version} />
          </div>
        </div>
      )}
    </div>
  );
}
