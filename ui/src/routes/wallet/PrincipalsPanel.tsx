import type { Identity } from "@/api/types";
import { Identifier } from "@/components/Identifier";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

/**
 * The principal set the identity document carries (contracts/README.md,
 * "Identity document"), read-only: one row per (identity, key, role). Inviting,
 * admitting and removing are ticket 028.
 */
export function PrincipalsPanel({ identity }: { identity: Identity }) {
  return (
    <Card data-testid="principals-panel">
      <CardHeader>
        <CardTitle>Principals</CardTitle>
        <CardDescription>identity id, key and role, one row per principal</CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {identity.principals.length === 0 ? (
          <p data-testid="principals-empty" className="text-sm">
            no principals recorded
          </p>
        ) : (
          <ul data-testid="principals-list" className="space-y-2">
            {identity.principals.map((principal) => (
              <li
                key={principal.identity}
                data-testid={`principal-row-${principal.identity}`}
                className="space-y-1"
              >
                <div className="flex flex-wrap items-center gap-2">
                  <Identifier value={principal.identity} />
                  <Badge data-testid={`principal-role-${principal.identity}`}>
                    {principal.role}
                  </Badge>
                  {principal.is_root && (
                    <Badge
                      variant="secondary"
                      data-testid={`principal-root-${principal.identity}`}
                    >
                      root
                    </Badge>
                  )}
                </div>
                <div className="text-xs text-muted-foreground">
                  active_key <Identifier value={principal.active_key} />
                </div>
              </li>
            ))}
          </ul>
        )}
        <p data-testid="principals-open-invitations" className="text-xs">
          open_invitation_count {identity.open_invitation_count}
        </p>
      </CardContent>
    </Card>
  );
}
