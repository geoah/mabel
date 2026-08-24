import { useMemo, useState } from "react";

import { listKnownIdentities } from "@/api/client";
import type { Identity, KnownIdentity } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import {
  factsFromResolved,
  type IdentityCardEntry,
  IdentityCardList,
  IdentityPillScope,
  type PillFacts,
} from "@/components/identity";
import { Section } from "@/components/Section";
import { Button } from "@/components/ui/button";
import { useResource } from "@/hooks/useResource";

/**
 * The standing note this list carries, which came off the witness route when
 * that route went away (proposal 006 section 8). A home that keeps records for
 * other people lists them here, and there is no global discovery: what is
 * missing here may be on another witness.
 */
export const HOLDINGS_NOTE =
  "This is what this home holds. A record missing here may still be on another witness.";

/** The resolved document behind one known row, which is what a card reads. */
function resolvedOf(row: KnownIdentity) {
  return {
    identity_id: row.identity_id,
    display_name: row.display_name,
    email: row.email,
    alias: row.alias,
    hostname: row.hostname,
    verification_status: row.verification_status,
    provenance: row.display_name ? ("profile" as const) : row.alias ? ("alias" as const) : ("none" as const),
  };
}

/**
 * True when this wallet has a reason to trust them: one of your identities said
 * so, or the stored crawl reaches them through other people. A row the crawl
 * never reached has no distance at all, which is the state the filter hides.
 */
export function isTrusted(row: KnownIdentity): boolean {
  return row.trusted || (row.degrees !== null && row.degrees > 0);
}

/** What the known rows tell the pills: no id here is one this wallet signs for. */
export function knownPills(rows: KnownIdentity[], own: Identity[]): PillFacts {
  const trusted = new Set<string>();
  const degrees = new Map<string, number>();
  for (const row of rows) {
    if (row.trusted) {
      trusted.add(row.identity_id);
    }
    if (row.degrees !== null) {
      degrees.set(row.identity_id, row.degrees);
    }
  }
  return { own: new Set(own.map((identity) => identity.identity_id)), trusted, degrees };
}

/**
 * Every identity this wallet has a record of and does not control: the ones it
 * fetched and the ones the last crawl read. The same card as everywhere, so a
 * name here reads exactly as it does on the identity's own page, and the toggle
 * on the heading narrows the list to the ones you have a reason to trust.
 */
export function KnownIdentities({ own }: { own: Identity[] }) {
  const known = useResource(listKnownIdentities, []);
  const [trustedOnly, setTrustedOnly] = useState(false);
  const rows = known.data?.identities ?? [];
  const shown = trustedOnly ? rows.filter(isTrusted) : rows;
  const entries: IdentityCardEntry[] = shown.map((row) => ({
    facts: factsFromResolved(resolvedOf(row), {
      declaredKind: row.declared_kind,
      stored: row.stored,
      headSeq: row.head_seq,
      to: `/identities/${row.identity_id}`,
    }),
  }));
  const pills = useMemo(
    () => knownPills(rows, own),
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [known.data, own],
  );

  return (
    <Section
      testId="known-identities"
      title="Known identities"
      description="Everyone your wallet has a record of and does not control."
      action={
        <Button
          type="button"
          variant={trustedOnly ? "default" : "outline"}
          size="sm"
          role="switch"
          aria-checked={trustedOnly}
          data-testid="known-trusted-only"
          onClick={() => setTrustedOnly(!trustedOnly)}
        >
          Trusted only
        </Button>
      }
    >
      <p data-testid="known-identities-note" className="text-sm text-muted-foreground">
        {HOLDINGS_NOTE}
      </p>
      {known.loading && <p data-testid="known-identities-loading">loading</p>}
      {known.error && (
        <ErrorEnvelopeView error={known.error} testId="known-identities-error" />
      )}
      {/* The distances come from the rows themselves, so no pill on this list
          costs a request of its own. */}
      {known.data && (
        <IdentityPillScope facts={pills}>
          <IdentityCardList
            entries={entries}
            testId="known-identity-cards"
            empty={
              trustedOnly
                ? "None of the identities your wallet knows of is trusted yet."
                : "Your wallet knows of no other identity yet."
            }
            emptyTestId="known-identities-empty"
          />
        </IdentityPillScope>
      )}
    </Section>
  );
}
