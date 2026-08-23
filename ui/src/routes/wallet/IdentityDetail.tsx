import { useCallback, useState } from "react";
import { Link, useParams } from "react-router";

import { getIdentity } from "@/api/client";
import { DeclaredKindNote, DeclaredKindValue } from "@/components/DeclaredKind";
import { DeveloperOnly, RawDocument } from "@/components/DeveloperMode";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { ResolvedIdentity, resolvedFrom } from "@/components/ResolvedIdentity";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

import { ContactPanel } from "./ContactPanel";
import { LedgerPanel } from "./LedgerPanel";
import { PrincipalsPanel } from "./PrincipalsPanel";
import { ProfilePanel } from "./ProfilePanel";
import { SyncPushPanel } from "./SyncPushPanel";
import { TrustPanel } from "./TrustPanel";
import { VerificationPanel } from "./VerificationPanel";
import { WitnessConfigPanel } from "./WitnessConfigPanel";

export function IdentityDetail() {
  const { identityId = "" } = useParams();
  const [version, setVersion] = useState(0);
  const identity = useResource(() => getIdentity(identityId), [identityId, version]);
  const refresh = useCallback(() => setVersion((value) => value + 1), []);
  const held = identity.data?.identity ?? null;

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
      {held && (
        <div className="grid gap-4 lg:grid-cols-2">
          <Card data-testid="identity-detail">
            <CardHeader>
              <CardTitle className="text-base">
                <ResolvedIdentity
                  identity={resolvedFrom(held)}
                  stale={held.verification.stale}
                  testId="identity-detail-resolved"
                />
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-2">
              {/* One compact table, key and value on a line (decision 014). */}
              <KeyValueTable>
                <KeyValue label="identity_id" testId="identity-detail-identity-id">
                  <Identifier value={held.identity_id} />
                </KeyValue>
                <KeyValue label="declared_kind" testId="identity-detail-declared-kind-row">
                  <DeclaredKindValue
                    kind={held.declared_kind}
                    testId="identity-detail-declared-kind"
                  />
                </KeyValue>
                <KeyValue label="alias" testId="identity-detail-alias">
                  {held.alias}
                </KeyValue>
                <KeyValue label="created_at_ms" testId="identity-detail-created-at-ms">
                  {held.created_at_ms}
                </KeyValue>
                <KeyValue label="hostname" testId="identity-detail-hostname">
                  {held.profile?.hostname ?? "none"}
                </KeyValue>
                <KeyValue label="contact" testId="identity-detail-contact">
                  {held.contact === null
                    ? "none"
                    : [held.contact.nickname, held.contact.note]
                        .filter((part) => part !== null)
                        .join(": ")}
                </KeyValue>
                <KeyValue label="events" testId="identity-detail-event-count">
                  {held.event_count}
                </KeyValue>
                <KeyValue label="people trusted" testId="identity-detail-trusted-count">
                  {held.trust.filter((record) => !record.revoked).length}
                </KeyValue>
                <KeyValue label="principals" testId="identity-detail-principal-count">
                  {held.principals.length}
                </KeyValue>
                <KeyValue label="open invitations" testId="identity-detail-open-invitations">
                  {held.open_invitation_count}
                </KeyValue>
                <KeyValue label="head_seq" testId="identity-detail-head-seq">
                  {held.head_seq}
                </KeyValue>
                <KeyValue label="active_key" testId="identity-detail-active-key">
                  <Identifier value={held.active_key} />
                </KeyValue>
                <KeyValue label="reserve_commit" testId="identity-detail-reserve-commit">
                  <Identifier value={held.reserve_commit} />
                </KeyValue>
                <DeveloperOnly>
                  <KeyValue label="head_event" testId="identity-detail-head-event">
                    <Identifier value={held.head_event} />
                  </KeyValue>
                </DeveloperOnly>
              </KeyValueTable>
              <DeclaredKindNote testId="identity-detail-declared-kind-note" />
              <RawDocument value={identity.data} testId="identity-detail-raw" />
            </CardContent>
          </Card>
          <ProfilePanel identity={held} onAppended={refresh} />
          <VerificationPanel identity={held} onChecked={refresh} />
          <ContactPanel
            identityId={held.identity_id}
            contact={held.contact}
            onSaved={refresh}
          />
          <PrincipalsPanel identity={held} />
          <WitnessConfigPanel identity={held} onAppended={refresh} />
          <TrustPanel identity={held} onAppended={refresh} />
          <SyncPushPanel identityId={held.identity_id} />
          <div className="lg:col-span-2">
            <LedgerPanel identityId={held.identity_id} version={version} />
          </div>
        </div>
      )}
    </div>
  );
}
