import { useCallback, useState } from "react";
import { Link, useParams } from "react-router";

import { getIdentity, getMemberships } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { useResolvedNames } from "@/hooks/useResolvedNames";
import { useResource } from "@/hooks/useResource";

import { ActionsSection } from "./ActionsSection";
import { LedgerPanel } from "./LedgerPanel";
import { OverviewCard } from "./OverviewCard";
import { PrincipalsPanel } from "./PrincipalsPanel";
import { TrustPanel, useTrustActions } from "./TrustPanel";

/**
 * One identity, in the four parts proposal 003 section 4 fixes: overview,
 * ledger, state, actions. Ticket 019's membership screens live in the actions
 * section and call the ticket 021 routes.
 */
export function IdentityDetail() {
  const { identityId = "" } = useParams();
  const [version, setVersion] = useState(0);
  const identity = useResource(() => getIdentity(identityId), [identityId, version]);
  const memberships = useResource(
    () => getMemberships(identityId),
    [identityId, version],
  );
  const refresh = useCallback(() => setVersion((value) => value + 1), []);
  const held = identity.data?.identity ?? null;

  // Every foreign id this page names: the subjects it trusts, the principals it
  // records and the identities it has invited. One lookup each names them.
  const foreign = [
    ...(held?.trust ?? []).map((record) => record.subject),
    ...(held?.principals ?? []).map((principal) => principal.identity),
    ...(memberships.data?.invitations ?? []).map((invitation) => invitation.invitee),
  ];
  const names = useResolvedNames(foreign, held?.identity_id ?? null);
  const trust = useTrustActions(identityId, refresh);

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
      {memberships.error && (
        <ErrorEnvelopeView error={memberships.error} testId="memberships-error" />
      )}
      {held && (
        <div className="grid gap-4 lg:grid-cols-2">
          <OverviewCard identity={held} raw={identity.data} />
          <LedgerPanel identityId={held.identity_id} version={version} />
          <TrustPanel identity={held} names={names} actions={trust} />
          <PrincipalsPanel
            identity={held}
            memberships={memberships.data}
            names={names}
          />
          <div className="lg:col-span-2">
            <ActionsSection
              identity={held}
              memberships={memberships.data}
              trust={trust}
              onAppended={refresh}
            />
          </div>
        </div>
      )}
    </div>
  );
}
