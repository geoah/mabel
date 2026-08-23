import { Link } from "react-router";

import type { Identity } from "@/api/types";
import {
  ResolvedIdentity,
  ResolvedIdentityScope,
  resolvedFrom,
} from "@/components/ResolvedIdentity";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { cn } from "@/lib/utils";
import { useSelectedIdentity } from "@/lib/preferences";

/**
 * The identity this wallet acts as, at the top of the wallet page (decision
 * 014). The choice is remembered in localStorage and is the default `from` of a
 * lookup; with nothing remembered the lowest identity id is selected, which is
 * what the node itself defaults to.
 */
export function IdentitySelector({ identities }: { identities: Identity[] }) {
  const [stored, setStored] = useSelectedIdentity();
  const resolved = identities.map(resolvedFrom);
  const selected =
    identities.find((identity) => identity.identity_id === stored) ?? identities[0] ?? null;

  return (
    <Card data-testid="identity-selector">
      <CardHeader>
        <CardTitle>Identity</CardTitle>
        <CardDescription>
          The identity this wallet acts as, and the default from of a lookup
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-2">
        {selected === null ? (
          <p data-testid="identity-selector-empty" className="text-sm">
            no identities in this node home
          </p>
        ) : (
          <ResolvedIdentityScope identities={resolved}>
            <p
              data-testid="identity-selector-selected"
              data-identity-id={selected.identity_id}
              className="sr-only"
            >
              {selected.identity_id}
            </p>
            <ul className="space-y-1" role="radiogroup" aria-label="identity">
              {identities.map((identity) => {
                const current = identity.identity_id === selected.identity_id;
                return (
                  <li
                    key={identity.identity_id}
                    data-testid={`identity-selector-row-${identity.identity_id}`}
                    className={cn(
                      "flex flex-wrap items-center gap-2 rounded-md px-2 py-1",
                      current && "bg-accent",
                    )}
                  >
                    <input
                      type="radio"
                      name="identity-selector"
                      value={identity.identity_id}
                      checked={current}
                      aria-label={identity.identity_id}
                      data-testid={`identity-selector-option-${identity.identity_id}`}
                      onChange={() => setStored(identity.identity_id)}
                    />
                    <ResolvedIdentity
                      identity={resolvedFrom(identity)}
                      stale={identity.verification.stale}
                      testId={`identity-selector-name-${identity.identity_id}`}
                    />
                    <Link
                      to={`/wallet/identities/${identity.identity_id}`}
                      data-testid={`identity-selector-open-${identity.identity_id}`}
                      className="ml-auto text-sm underline"
                    >
                      Open
                    </Link>
                  </li>
                );
              })}
            </ul>
          </ResolvedIdentityScope>
        )}
      </CardContent>
    </Card>
  );
}
