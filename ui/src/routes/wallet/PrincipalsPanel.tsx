import type {
  Identity,
  MembershipView,
  ResolvedIdentity as ResolvedIdentityDocument,
} from "@/api/types";
import { ResolvedIdentity } from "@/components/ResolvedIdentity";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { bareIdentity } from "@/hooks/useResolvedNames";

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
  names: Map<string, ResolvedIdentityDocument>;
}) {
  const invitations = memberships?.invitations ?? [];
  const resolved = (id: string) => names.get(id) ?? bareIdentity(id);

  if (identity.principals.length <= 1 && invitations.length === 0) {
    return null;
  }

  return (
    <Card data-testid="principals-panel">
      <CardHeader>
        <CardTitle>Who can act for this identity</CardTitle>
        <CardDescription>
          Everyone allowed to sign for it, and every invitation it has sent
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <ul data-testid="principals-list" className="divide-y">
          {identity.principals.map((principal) => (
            <li
              key={principal.identity}
              data-testid={`principal-row-${principal.identity}`}
              className="flex flex-wrap items-center gap-x-2 gap-y-1 py-2"
            >
              <ResolvedIdentity
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
                <ResolvedIdentity
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
                <span className="text-xs text-muted-foreground">
                  invited at position {invitation.invitation_seq}
                </span>
              </li>
            ))}
          </ul>
        )}
        <p data-testid="principals-open-invitations" className="text-xs text-muted-foreground">
          {identity.open_invitation_count === 0
            ? "No invitation is waiting to be accepted."
            : `${identity.open_invitation_count} ${
                identity.open_invitation_count === 1 ? "invitation is" : "invitations are"
              } still waiting to be accepted.`}
        </p>
      </CardContent>
    </Card>
  );
}
