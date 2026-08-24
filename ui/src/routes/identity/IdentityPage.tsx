import { useCallback, useMemo, useState } from "react";
import { useParams, useSearchParams } from "react-router";

import {
  ApiError,
  getContact,
  getIdentity,
  getMemberships,
  listIdentities,
  listWitnesses,
  lookup,
} from "@/api/client";
import type { IdentityResponse, LookupResponse, MembershipView } from "@/api/types";
import { Action } from "@/components/Action";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  bareIdentity,
  factsFromIdentity,
  factsFromResolved,
  IdentityCard,
  IdentityPillScope,
  machinesOf,
  pageTestIds,
  type PillFacts,
  trustedSubjects,
} from "@/components/identity";
import { PageSections } from "@/components/Section";
import { degreesOf, named, useResolvedNames } from "@/hooks/useResolvedNames";
import { useResource } from "@/hooks/useResource";
import { ActionsSection } from "@/routes/wallet/ActionsSection";
import { ContactPanel } from "@/routes/wallet/ContactPanel";
import { LedgerPanel } from "@/routes/wallet/LedgerPanel";
import { PrincipalsPanel } from "@/routes/wallet/PrincipalsPanel";
import { TrustPanel, useTrustActions } from "@/routes/wallet/TrustPanel";
import { WitnessHoldings } from "@/routes/witnesses/WitnessHoldings";

import { FetchButton, FetchPanel } from "./FetchPanel";
import { KnowledgeSection } from "./KnowledgeSection";

/**
 * The query key a pasted link leaves behind: the machines it named, so the
 * fetch on this page can dial them. The browser parsed nothing to get them, the
 * resolve route did (proposal 006 section 7).
 */
export const LINK_MACHINES_PARAM = "machines";

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
    // One row on its own, so it takes the rule its siblings in a group get.
    <div className="border-t">
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
    </div>
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
  const [search] = useSearchParams();
  const [version, setVersion] = useState(0);
  const refresh = useCallback(() => setVersion((value) => value + 1), []);
  // The machines a pasted link named, in the order the link carried them.
  const hinted = (search.get(LINK_MACHINES_PARAM) ?? "").split(",").filter(Boolean);

  const identities = useResource(listIdentities, [version]);
  const witnesses = useResource(listWitnesses, [version]);
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
  // GET /api/identities lists exactly the ledgers this home can sign for, which
  // is the only answer the identity document gives about control.
  const canSign = listed;
  // A witness is an identity, so this is its page: the row about the machines
  // that answer for it and the section about what it holds are drawn here, and
  // only when this home knows it as a witness.
  const witness =
    witnesses.data?.witnesses.find((entry) => entry.identity_id === identityId) ?? null;
  const machines = machinesOf(held, witness);
  // The witness list is loaded for the section below, so the witnesses this
  // identity chose are named from it rather than from a request each.
  const nameWitness = (identityId: string) => {
    const entry = witnesses.data?.witnesses.find((row) => row.identity_id === identityId);
    return { ...bareIdentity(identityId), display_name: entry?.display_name ?? null };
  };

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
      <PageSections>
        {loading && <p data-testid="identity-detail-loading">loading</p>}
        {identity.error && <ErrorEnvelopeView error={identity.error} testId="identity-detail-error" />}
        {memberships.error && (
          <ErrorEnvelopeView error={memberships.error} testId="memberships-error" />
        )}
        {held && (
          <>
            <IdentityCard
              facts={{ ...factsFromIdentity(held), machines }}
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
                    machines={hinted}
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
            // This page asked for the record and was told there is none, so the
            // card is the one place that can say so for certain.
            facts={factsFromResolved(answer.identity, {
              stale: answer.stale,
              stored: false,
              machines,
            })}
            state="page"
            testIds={pageTestIds}
          />
        )}
        {/* A witness is an identity, and what it keeps for other people is a
            fact about it, so it lands on its own page. */}
        {witness && <WitnessHoldings witness={witness} own={localIdentities} />}
        {knowledge.error && <ErrorEnvelopeView error={knowledge.error} testId="lookup-error" />}
        {!canSign && (
          <>
            <ContactSection identityId={identityId} />
            {answer && <KnowledgeSection response={answer} onSynced={knowledge.reload} />}
            {from === null && !identity.loading && (
              <p data-testid="lookup-no-root" className="text-sm">
                Your wallet holds no identity of its own to answer from.
              </p>
            )}
          </>
        )}
        {held === null && !identity.loading && !identity.error && (
          <FetchPanel identityId={identityId} onFetched={refresh} machines={hinted} />
        )}
        {canSign && held && (
          <ActionsSection
            identity={held}
            memberships={memberships.data}
            trust={trust}
            machines={machines}
            names={nameWitness}
            onAppended={refresh}
          />
        )}
      </PageSections>
    </IdentityPillScope>
  );
}
