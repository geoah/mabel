import type { Identity } from "@/api/types";
import { DeclaredKindNote, DeclaredKindValue } from "@/components/DeclaredKind";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import {
  ResolvedIdentity,
  VerificationMark,
  VerificationNote,
  resolvedFrom,
} from "@/components/ResolvedIdentity";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { formatDate } from "@/lib/time";

/**
 * The overview of proposal 003 section 4: one compact key-value table, key and
 * value on a line and never stacked, holding what an address book entry is.
 *
 * The two roots differ in one fact a reader can act on: an identity either
 * holds a key of its own or is signed for by its controllers. That is a
 * sentence, not two 52-character values, so the row says it in words
 * (decision 017).
 */
export function OverviewCard({
  identity,
  own = false,
}: {
  identity: Identity;
  /** True when this wallet holds a key that may sign for the identity. */
  own?: boolean;
}) {
  const contact = identity.contact;
  const trusted = identity.trust.filter((record) => !record.revoked).length;

  return (
    <Card data-testid="identity-detail">
      <CardHeader>
        <CardTitle className="flex flex-wrap items-baseline gap-2 text-base">
          <ResolvedIdentity
            identity={resolvedFrom(identity)}
            stale={identity.verification.stale}
            testId="identity-detail-resolved"
          />
          {/* Beside the name, where it reads as a fact about the identity. */}
          {own && <Badge data-testid="identity-own-badge">your identity</Badge>}
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-2">
        <KeyValueTable>
          <KeyValue label="identity id" testId="identity-detail-identity-id">
            <Identifier value={identity.identity_id} />
          </KeyValue>
          <KeyValue label="declared kind" testId="identity-detail-declared-kind-row">
            <DeclaredKindValue
              kind={identity.declared_kind}
              testId="identity-detail-declared-kind"
            />
          </KeyValue>
          <KeyValue label="your name for it" testId="identity-detail-alias">
            {identity.alias}
          </KeyValue>
          <KeyValue label="created" testId="identity-detail-created">
            {formatDate(identity.created_at_ms)}
          </KeyValue>
          <KeyValue label="website" testId="identity-detail-hostname">
            {identity.verification.hostname === null ? (
              "none"
            ) : (
              <VerificationMark
                status={identity.verification.status}
                hostname={identity.verification.hostname}
                stale={identity.verification.stale}
                testId="identity-detail-hostname-verification"
              />
            )}
          </KeyValue>
          <KeyValue label="your private note" testId="identity-detail-contact">
            {contact === null
              ? "none"
              : [contact.nickname, contact.note].filter((part) => part !== null).join(": ")}
          </KeyValue>
          <KeyValue label="record" testId="identity-detail-ledger-summary">
            <span data-testid="identity-detail-event-count">{identity.event_count}</span>{" "}
            {identity.event_count === 1 ? "entry" : "entries"}, the newest at position{" "}
            <span data-testid="identity-detail-head-seq">{identity.head_seq}</span>
          </KeyValue>
          <KeyValue label="trusts" testId="identity-detail-trusted-count">
            {trusted} {trusted === 1 ? "identity" : "identities"}
          </KeyValue>
          <KeyValue label="who can act for it" testId="identity-detail-principal-count">
            {identity.principals.length}
          </KeyValue>
          <KeyValue label="invitations waiting" testId="identity-detail-open-invitations">
            {identity.open_invitation_count}
          </KeyValue>
          <KeyValue label="keys" testId="identity-detail-keys">
            {identity.active_key
              ? "this identity signs with a key of its own, and holds a spare to replace it with"
              : "this identity holds no key of its own; its controllers sign for it"}
          </KeyValue>
        </KeyValueTable>
        <DeclaredKindNote testId="identity-detail-declared-kind-note" />
        {identity.verification.hostname !== null && (
          <VerificationNote testId="identity-detail-verification-note" />
        )}
      </CardContent>
    </Card>
  );
}
