import { useMemo } from "react";

import { listIdentities } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  factsFromIdentity,
  type IdentityCardEntry,
  IdentityCardList,
  IdentityPillScope,
  type PillFacts,
  trustedSubjects,
} from "@/components/identity";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleChevron,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { useResource } from "@/hooks/useResource";

import { IdentityCreateForm } from "./IdentityCreateForm";
import { WalletSearch } from "./WalletSearch";

/**
 * The wallet front page (proposal 004): one box to open an identity, the
 * identities this home holds as cards, and the create form folded away. There
 * is no selection: an identity is a page, not a mode the wallet is in.
 *
 * Every card here is an identity this wallet signs for, so every one carries the
 * "your identity" pill. No distance is known on this screen and no request runs
 * to find one out, so nothing here wears an orange pill (proposal 005).
 */
export function WalletHome() {
  const identities = useResource(listIdentities, []);
  const held = identities.data?.identities ?? [];
  const entries: IdentityCardEntry[] = held.map((identity) => ({
    facts: factsFromIdentity(identity, `/identities/${identity.identity_id}`),
  }));
  const pills = useMemo<PillFacts>(
    () => ({
      own: new Set(held.map((identity) => identity.identity_id)),
      trusted: trustedSubjects(held),
      degrees: new Map<string, number>(),
    }),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [identities.data],
  );

  return (
    <IdentityPillScope facts={pills}>
      <div className="space-y-4">
        <WalletSearch />
        <Card data-testid="identity-list">
          <CardHeader>
            <CardTitle>Your identities</CardTitle>
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
                empty="You have no identities yet. Create one below."
                emptyTestId="identity-list-empty"
              />
            )}
          </CardContent>
        </Card>
        <Collapsible data-testid="identity-create" className="rounded-lg border bg-card">
          <CollapsibleTrigger
            data-testid="identity-create-summary"
            className="flex w-full min-h-11 items-center gap-2 px-3 text-left text-sm font-medium hover:bg-accent sm:px-4"
          >
            <CollapsibleChevron />
            Create an identity
          </CollapsibleTrigger>
          <CollapsibleContent className="border-t p-3 sm:p-4">
            <IdentityCreateForm onCreated={identities.reload} />
          </CollapsibleContent>
        </Collapsible>
      </div>
    </IdentityPillScope>
  );
}
