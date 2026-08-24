import { useMemo } from "react";

import { listIdentities } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  bareIdentity,
  factsFromIdentity,
  type IdentityCardEntry,
  IdentityCardList,
  IdentityPillScope,
  type PillFacts,
  resolvedFrom,
  trustedSubjects,
} from "@/components/identity";
import {
  Collapsible,
  CollapsibleChevron,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { PageSections, Section } from "@/components/Section";
import { useResource } from "@/hooks/useResource";

import { IdentityCreateForm } from "./IdentityCreateForm";
import { KnownIdentities } from "./KnownIdentities";
import { WalletSearch } from "./WalletSearch";

/**
 * The wallet front page (proposal 004): three flat sections, each under its own
 * heading. Open an identity, the identities this wallet signs for, and the
 * identities it knows of and does not control. No section is a card, because the
 * cards are the identities.
 *
 * Every card in "Your identities" carries the "your identity" pill. Distance is
 * a fact the known rows carry, so the second list is the only one with orange on
 * it and no screen here fires a request for a pill (proposal 005).
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
  // A controller of one identity here is usually another identity here, so the
  // open card names it instead of printing a 52-character id.
  const resolvePrincipal = useMemo(() => {
    const table = new Map(held.map((identity) => [identity.identity_id, resolvedFrom(identity)]));
    return (identityId: string) => table.get(identityId) ?? bareIdentity(identityId);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [identities.data]);

  return (
    <IdentityPillScope facts={pills}>
      <PageSections>
        <Section title="Open an identity">
          <WalletSearch />
        </Section>
        <Section testId="identity-list" title="Your identities">
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
              resolvePrincipal={resolvePrincipal}
            />
          )}
          {/* A row, not a card: the cards in this section are the identities. */}
          <Collapsible data-testid="identity-create" className="border-t">
            <CollapsibleTrigger
              data-testid="identity-create-summary"
              className="flex w-full min-h-11 items-center gap-2 rounded-md px-1 text-left text-sm font-medium hover:bg-accent"
            >
              <CollapsibleChevron />
              Create an identity
            </CollapsibleTrigger>
            <CollapsibleContent className="pb-3 pl-6">
              <IdentityCreateForm onCreated={identities.reload} />
            </CollapsibleContent>
          </Collapsible>
        </Section>
        <KnownIdentities own={held} />
      </PageSections>
    </IdentityPillScope>
  );
}
