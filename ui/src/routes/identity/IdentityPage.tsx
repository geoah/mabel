import { useCallback, useState } from "react";
import { Link, useParams } from "react-router";

import {
  ApiError,
  getContact,
  getIdentity,
  getMemberships,
  listIdentities,
  lookup,
} from "@/api/client";
import type {
  IdentityResponse,
  LookupResponse,
  MembershipView,
  NameProvenance,
} from "@/api/types";
import { Action } from "@/components/Action";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { ResolvedIdentity } from "@/components/ResolvedIdentity";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useResolvedNames } from "@/hooks/useResolvedNames";
import { useResource } from "@/hooks/useResource";
import { ActionsSection } from "@/routes/wallet/ActionsSection";
import { ContactPanel } from "@/routes/wallet/ContactPanel";
import { LedgerPanel } from "@/routes/wallet/LedgerPanel";
import { OverviewCard } from "@/routes/wallet/OverviewCard";
import { PrincipalsPanel } from "@/routes/wallet/PrincipalsPanel";
import { TrustPanel, useTrustActions } from "@/routes/wallet/TrustPanel";

import { FetchButton, FetchPanel } from "./FetchPanel";
import { KnowledgeSection } from "./KnowledgeSection";

/** A record this wallet does not hold is an answer, not a failure. */
function notStored(thrown: unknown): null {
  if (thrown instanceof ApiError && (thrown.status === 404 || thrown.reason === "unknown_ledger")) {
    return null;
  }
  throw thrown;
}

/**
 * The private note this home keeps about a foreign identity. The contact store
 * covers ids whose ledger this wallet does not hold, which is the point of it
 * (proposal 003 section 1).
 */
function ContactSection({ identityId }: { identityId: string }) {
  const [version, setVersion] = useState(0);
  const contact = useResource(() => getContact(identityId), [identityId, version]);

  return (
    <Action
      testId="lookup-contact"
      title="Write a private note"
      description="A nickname and note only you see. It stays on this computer and is never published."
    >
      {contact.error && <ErrorEnvelopeView error={contact.error} testId="lookup-contact-error" />}
      {contact.data && (
        <ContactPanel
          identityId={identityId}
          contact={contact.data.contact}
          onSaved={() => setVersion((value) => value + 1)}
        />
      )}
    </Action>
  );
}

/** Where the name on a crawled page came from, in the order section 4 fixes. */
const PROVENANCE_SENTENCE: Record<NameProvenance, string> = {
  profile: "the name they publish themselves",
  alias: "your own nickname for them, which nobody else sees",
  none: "nothing your wallet knows, so the id is the only label",
};

/** The overview of a record this wallet does not hold: what it found them called. */
function CrawledOverview({ answer }: { answer: LookupResponse }) {
  return (
    <Card data-testid="identity-detail">
      <CardHeader>
        <CardTitle className="text-base">
          <ResolvedIdentity identity={answer.identity} testId="identity-detail-resolved" />
        </CardTitle>
      </CardHeader>
      <CardContent>
        <KeyValueTable>
          <KeyValue label="identity id" testId="identity-detail-identity-id">
            <Identifier value={answer.identity.identity_id} />
          </KeyValue>
          <KeyValue label="name comes from" testId="identity-detail-provenance">
            {PROVENANCE_SENTENCE[answer.identity.provenance]}
          </KeyValue>
          <KeyValue label="record" testId="identity-detail-ledger-summary">
            your wallet holds no copy of it
          </KeyValue>
        </KeyValueTable>
      </CardContent>
    </Card>
  );
}

/**
 * One identity, local or foreign, stored or not (proposal 004). What varies is
 * a single fact: when this wallet can sign for the record the overview card
 * carries the "your identity" badge and the page carries the actions; otherwise
 * it carries the private note and how you know them. Everything else renders
 * from whatever the wallet holds.
 */
export function IdentityPage() {
  const { identityId = "" } = useParams();
  const [version, setVersion] = useState(0);
  const refresh = useCallback(() => setVersion((value) => value + 1), []);

  const identities = useResource(listIdentities, [version]);
  const identity = useResource<IdentityResponse | null>(
    () => getIdentity(identityId).catch(notStored),
    [identityId, version],
  );
  const memberships = useResource<MembershipView | null>(
    () => getMemberships(identityId).catch(notStored),
    [identityId, version],
  );

  const localIdentities = identities.data?.identities ?? [];
  // GET /api/identities is sorted by ascending identity id, which is the root
  // the node itself defaults a lookup to.
  const from = localIdentities[0]?.identity_id ?? null;
  const listed = localIdentities.some((entry) => entry.identity_id === identityId);
  const held = identity.data?.identity ?? null;
  const declaredControl = held?.controlled_by;
  const canSign = declaredControl === undefined ? listed : declaredControl !== null;

  const knowledge = useResource<LookupResponse | null>(
    () =>
      canSign || from === null || from === identityId
        ? Promise.resolve(null)
        : lookup(identityId, { from }),
    [identityId, from, canSign, version],
  );

  // Every foreign id this page names: the subjects it trusts, the principals it
  // records and the identities it has invited. One lookup each names them.
  const foreign = [
    ...(held?.trust ?? []).map((record) => record.subject),
    ...(held?.principals ?? []).map((principal) => principal.identity),
    ...(memberships.data?.invitations ?? []).map((invitation) => invitation.invitee),
  ];
  const names = useResolvedNames(foreign, from);
  const trust = useTrustActions(identityId, refresh);
  const loading = identity.loading || identities.loading;

  return (
    <div className="space-y-4">
      {/* The back link is navigation and nothing else lives in its row. */}
      <Link
        to="/wallet"
        className="inline-flex min-h-10 items-center text-sm underline"
        data-testid="identity-back"
      >
        Wallet
      </Link>
      {loading && <p data-testid="identity-detail-loading">loading</p>}
      {identity.error && (
        <ErrorEnvelopeView error={identity.error} testId="identity-detail-error" />
      )}
      {memberships.error && (
        <ErrorEnvelopeView error={memberships.error} testId="memberships-error" />
      )}
      {held && (
        <div className="grid gap-4 lg:grid-cols-2">
          <OverviewCard identity={held} own={canSign} />
          <LedgerPanel
            identityId={held.identity_id}
            version={version}
            // A record this wallet signs for is never missing its own entries,
            // so only a stored foreign one offers to fetch the rest.
            fetch={
              canSign ? undefined : (
                <FetchButton
                  identityId={held.identity_id}
                  onFetched={refresh}
                  testId="ledger-fetch-button"
                />
              )
            }
          />
          <TrustPanel identity={held} names={names} actions={trust} />
          <PrincipalsPanel identity={held} memberships={memberships.data} names={names} />
        </div>
      )}
      {held === null && knowledge.data && <CrawledOverview answer={knowledge.data} />}
      {knowledge.error && (
        <ErrorEnvelopeView error={knowledge.error} testId="lookup-error" />
      )}
      {!canSign && (
        <>
          <ContactSection identityId={identityId} />
          {knowledge.data && <KnowledgeSection response={knowledge.data} />}
          {from === null && !identity.loading && (
            <p data-testid="lookup-no-root" className="text-sm">
              Your wallet holds no identity of its own to answer from.
            </p>
          )}
        </>
      )}
      {held === null && !identity.loading && !identity.error && (
        <FetchPanel identityId={identityId} onFetched={refresh} />
      )}
      {canSign && held && (
        <ActionsSection
          identity={held}
          memberships={memberships.data}
          trust={trust}
          onAppended={refresh}
        />
      )}
    </div>
  );
}
