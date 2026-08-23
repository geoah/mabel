import type { Identity } from "@/api/types";
import { DeclaredKindNote, DeclaredKindValue } from "@/components/DeclaredKind";
import { DeveloperOnly, RawDocument } from "@/components/DeveloperMode";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import {
  ResolvedIdentity,
  VerificationMark,
  VerificationNote,
  resolvedFrom,
} from "@/components/ResolvedIdentity";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { formatDate } from "@/lib/time";

/**
 * The overview of proposal 003 section 4: one compact key-value table, key and
 * value on a line and never stacked, holding what an address book entry is.
 * Sequencing, event ids and cache freshness are developer mode's (decision
 * 014); `active_key` and `reserve_commit` stay because a raw root carrying them
 * and an identity root carrying neither is the difference between two kinds of
 * ledger, not a detail.
 */
export function OverviewCard({
  identity,
  raw,
}: {
  identity: Identity;
  /** The whole response document, which developer mode prints verbatim. */
  raw: unknown;
}) {
  const contact = identity.contact;

  return (
    <Card data-testid="identity-detail">
      <CardHeader>
        <CardTitle className="text-base">
          <ResolvedIdentity
            identity={resolvedFrom(identity)}
            stale={identity.verification.stale}
            testId="identity-detail-resolved"
          />
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-2">
        <KeyValueTable>
          <KeyValue label="identity_id" testId="identity-detail-identity-id">
            <Identifier value={identity.identity_id} />
          </KeyValue>
          <KeyValue label="declared kind" testId="identity-detail-declared-kind-row">
            <DeclaredKindValue
              kind={identity.declared_kind}
              testId="identity-detail-declared-kind"
            />
          </KeyValue>
          <KeyValue label="alias" testId="identity-detail-alias">
            {identity.alias}
          </KeyValue>
          <KeyValue label="created" testId="identity-detail-created">
            {formatDate(identity.created_at_ms)}
          </KeyValue>
          <KeyValue label="hostname" testId="identity-detail-hostname">
            {identity.verification.hostname === null ? (
              "none claimed"
            ) : (
              <VerificationMark
                status={identity.verification.status}
                hostname={identity.verification.hostname}
                stale={identity.verification.stale}
                testId="identity-detail-hostname-verification"
              />
            )}
          </KeyValue>
          <KeyValue label="contact" testId="identity-detail-contact">
            {contact === null
              ? "none"
              : [contact.nickname, contact.note].filter((part) => part !== null).join(": ")}
          </KeyValue>
          <KeyValue label="ledger" testId="identity-detail-ledger-summary">
            <span data-testid="identity-detail-event-count">{identity.event_count}</span> events,
            head at seq <span data-testid="identity-detail-head-seq">{identity.head_seq}</span>
          </KeyValue>
          <KeyValue label="people trusted" testId="identity-detail-trusted-count">
            {identity.trust.filter((record) => !record.revoked).length}
          </KeyValue>
          <KeyValue label="principals" testId="identity-detail-principal-count">
            {identity.principals.length}
          </KeyValue>
          <KeyValue label="open invitations" testId="identity-detail-open-invitations">
            {identity.open_invitation_count}
          </KeyValue>
          <KeyValue label="active_key" testId="identity-detail-active-key">
            <Identifier value={identity.active_key} />
          </KeyValue>
          <KeyValue label="reserve_commit" testId="identity-detail-reserve-commit">
            <Identifier value={identity.reserve_commit} />
          </KeyValue>
          <DeveloperOnly>
            <KeyValue label="head_event" testId="identity-detail-head-event">
              <Identifier value={identity.head_event} />
            </KeyValue>
            <KeyValue label="created_at_ms" testId="identity-detail-created-at-ms">
              {identity.created_at_ms}
            </KeyValue>
          </DeveloperOnly>
        </KeyValueTable>
        <DeclaredKindNote testId="identity-detail-declared-kind-note" />
        {identity.verification.hostname !== null && (
          <VerificationNote testId="identity-detail-verification-note" />
        )}
        <RawDocument value={raw} testId="identity-detail-raw" />
      </CardContent>
    </Card>
  );
}
