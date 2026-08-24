import { useCallback, useMemo, useState } from "react";
import { useParams } from "react-router";

import { ApiError, getContact, getIdentity, getMemberships, listIdentities, lookup } from "@/api/client";
import type { IdentityResponse, LookupResponse, MembershipView } from "@/api/types";
import { Action } from "@/components/Action";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  factsFromIdentity,
  factsFromResolved,
  IdentityCard,
  IdentityPillScope,
  pageTestIds,
  type PillFacts,
  trustedSubjects,
} from "@/components/identity";
import { degreesOf, named, useResolvedNames } from "@/hooks/useResolvedNames";
import { useResource } from "@/hooks/useResource";
import { ActionsSection } from "@/routes/wallet/ActionsSection";
import { ContactPanel } from "@/routes/wallet/ContactPanel";
import { LedgerPanel } from "@/routes/wallet/LedgerPanel";
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
 * The nickname and note this home keeps about a foreign identity. The contact
 * store covers ids whose ledger this wallet does not hold, which is the point of
 * it (proposal 003 section 1).
 */
function ContactSection({ identityId }: { identityId: string }) {
  const [version, setVersion] = useState(0);
  const contact = useResource(() => getContact(identityId), [identityId, version]);

  return (
    <Action
      testId="lookup-contact"
      title="Update local info"
      description="The nickname and note only this device sees."
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

/**
 * One identity, local or foreign, stored or not (proposal 004). What varies is
 * a single fact: when this wallet can sign for the record the card carries the
 * "your identity" pill and the page carries the actions; otherwise it carries
 * the private note and how you know them. Everything else renders from whatever
 * the wallet holds, through the same card a list draws (proposal 005).
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
  const answer = knowledge.data;

  /**
   * What the pills on this page read, all of it from documents already loaded:
   * the identities this home holds, their unrevoked attestations, and the
   * distances the lookups on this page reported. No request runs for a pill.
   */
  const pills = useMemo<PillFacts>(() => {
    const own = new Set(localIdentities.map((entry) => entry.identity_id));
    if (canSign) {
      own.add(identityId);
    }
    const degrees = degreesOf(names);
    if (answer?.degrees !== null && answer?.degrees !== undefined) {
      degrees.set(answer.identity.identity_id, answer.degrees);
    }
    return { own, trusted: trustedSubjects(localIdentities), degrees };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [identities.data, canSign, identityId, names, answer]);

  return (
    <IdentityPillScope facts={pills}>
      <div className="space-y-4">
        {loading && <p data-testid="identity-detail-loading">loading</p>}
        {identity.error && <ErrorEnvelopeView error={identity.error} testId="identity-detail-error" />}
        {memberships.error && (
          <ErrorEnvelopeView error={memberships.error} testId="memberships-error" />
        )}
        {held && (
          <>
            <IdentityCard
              facts={factsFromIdentity(held)}
              state="page"
              testIds={pageTestIds}
              resolvePrincipal={(principal) => named(names, principal)}
            />
            {/* Who they trust comes before the record: it is what a reader of an
                address book came for, and the record is the evidence under it. */}
            <TrustPanel identity={held} names={names} actions={trust} />
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
            <PrincipalsPanel identity={held} memberships={memberships.data} names={names} />
          </>
        )}
        {held === null && answer && (
          <IdentityCard
            facts={factsFromResolved(answer.identity, { stale: answer.stale })}
            state="page"
            testIds={pageTestIds}
          />
        )}
        {knowledge.error && <ErrorEnvelopeView error={knowledge.error} testId="lookup-error" />}
        {!canSign && (
          <>
            <ContactSection identityId={identityId} />
            {answer && <KnowledgeSection response={answer} />}
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
    </IdentityPillScope>
  );
}
