import type { Identity, MembershipView } from "@/api/types";
import { IdentityInline, IdentityListScope } from "@/components/identity";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { named, type ResolvedNames } from "@/hooks/useResolvedNames";

/**
 * Who may act on this ledger, and every invitation it ever issued. A ledger
 * with nothing but its root principal and no invitation has nothing to say
 * here, so the card is not drawn at all (proposal 003 section 4).
 */
export function PrincipalsPanel({
  identity,
  memberships,
  names,
}: {
  identity: Identity;
  /** GET /api/identities/:identity_id/memberships, null while it is in flight. */
  memberships: MembershipView | null;
  names: ResolvedNames;
}) {
  const invitations = memberships?.invitations ?? [];
  const resolved = (id: string) => named(names, id);
  // An identity-rooted ledger holds no key of its own. That is the one fact
  // about keys a reader acts on, and this is the one place it is said.
  const founded = identity.active_key === undefined;

  if (identity.principals.length <= 1 && invitations.length === 0) {
    return null;
  }

  return (
    <Card data-testid="principals-panel">
      <CardHeader>
        <CardTitle>Who can act for this identity</CardTitle>
        <CardDescription data-testid="principals-description">
          Everyone allowed to sign for it, and every invitation it has sent
          {founded ? ". Its controllers sign for it." : ""}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <IdentityListScope
          identities={[
            ...identity.principals.map((principal) => resolved(principal.identity)),
            ...invitations.map((invitation) => resolved(invitation.invitee)),
          ]}
        >
          <ul data-testid="principals-list" className="divide-y">
            {identity.principals.map((principal) => (
              <li
                key={principal.identity}
                data-testid={`principal-row-${principal.identity}`}
                className="flex flex-wrap items-center gap-x-2 gap-y-1 py-2"
              >
                <IdentityInline
                  identity={resolved(principal.identity)}
                  testId={`principal-name-${principal.identity}`}
                  to={`/identities/${principal.identity}`}
                />
                <Badge data-testid={`principal-role-${principal.identity}`}>{principal.role}</Badge>
                {principal.is_root && (
                  <Badge variant="secondary" data-testid={`principal-root-${principal.identity}`}>
                    founder
                  </Badge>
                )}
              </li>
            ))}
          </ul>
          {invitations.length > 0 && (
            <ul data-testid="invitations-list" className="divide-y border-t">
              {invitations.map((invitation) => (
                <li
                  key={invitation.invitation_event}
                  data-testid={`invitation-row-${invitation.invitee}`}
                  className="flex flex-wrap items-center gap-x-2 gap-y-1 py-2"
                >
                  <IdentityInline
                    identity={resolved(invitation.invitee)}
                    testId={`invitation-name-${invitation.invitee}`}
                    to={`/identities/${invitation.invitee}`}
                  />
                  <Badge variant="outline" data-testid={`invitation-role-${invitation.invitee}`}>
                    {invitation.role}
                  </Badge>
                  <Badge
                    variant={invitation.status === "open" ? "secondary" : "outline"}
                    data-testid={`invitation-status-${invitation.invitee}`}
                  >
                    {invitation.status}
                  </Badge>
                </li>
              ))}
            </ul>
          )}
        </IdentityListScope>
        <p data-testid="principals-open-invitations" className="text-xs text-muted-foreground">
          {identity.open_invitation_count === 0
            ? "No invitation to help control this identity is waiting for an answer."
            : `${identity.open_invitation_count} ${
                identity.open_invitation_count === 1 ? "invitation" : "invitations"
              } to help control this identity ${
                identity.open_invitation_count === 1 ? "is" : "are"
              } still waiting for an answer.`}
        </p>
      </CardContent>
    </Card>
  );
}
