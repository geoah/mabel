import { listIdentities } from "@/api/client";
import { DeclaredKindNote } from "@/components/DeclaredKind";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { type IdentityCardEntry, IdentityCardList } from "@/components/IdentityCardList";
import { resolvedFrom } from "@/components/ResolvedIdentity";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

import { IdentityCreateForm } from "./IdentityCreateForm";
import { WalletSearch } from "./WalletSearch";

/**
 * The wallet front page (proposal 004): one box to open an identity, the
 * identities this home holds as cards, and the create form folded away. There
 * is no selection: an identity is a page, not a mode the wallet is in.
 */
export function WalletHome() {
  const identities = useResource(listIdentities, []);
  const entries: IdentityCardEntry[] = (identities.data?.identities ?? []).map((identity) => ({
    identity: resolvedFrom(identity),
    declaredKind: identity.declared_kind,
    headSeq: identity.head_seq,
    stale: identity.verification.stale,
    to: `/identities/${identity.identity_id}`,
  }));

  return (
    <div className="space-y-4">
      <WalletSearch />
      <Card data-testid="identity-list">
        <CardHeader>
          <CardTitle>Identities</CardTitle>
          <DeclaredKindNote testId="identity-list-declared-kind-note" />
        </CardHeader>
        <CardContent className="space-y-3">
          {identities.loading && <p data-testid="identity-list-loading">loading</p>}
          {identities.error && (
            <ErrorEnvelopeView error={identities.error} testId="identity-list-error" />
          )}
          {identities.data && (
            <IdentityCardList
              entries={entries}
              testId="identity-cards"
              empty="no identities in this node home"
              emptyTestId="identity-list-empty"
            />
          )}
        </CardContent>
      </Card>
      <details data-testid="identity-create" className="rounded-lg border bg-card">
        <summary
          data-testid="identity-create-summary"
          className="flex min-h-11 cursor-pointer list-none items-center px-3 text-sm font-medium marker:content-none hover:bg-accent sm:px-4"
        >
          New identity
        </summary>
        <div className="border-t p-3 sm:p-4">
          <IdentityCreateForm onCreated={identities.reload} />
        </div>
      </details>
    </div>
  );
}
