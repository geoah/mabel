import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

/**
 * The principal set of proposal 002 section 1, one row per (identity id, key,
 * role). The membership surface is not frozen (contracts/http/PENDING-membership.md)
 * so the identity document carries no principals field yet. Ticket 019 fills this.
 */
export function PrincipalsPanel() {
  return (
    <Card data-testid="principals-panel">
      <CardHeader>
        <CardTitle>Principals</CardTitle>
        <CardDescription>identity id, key and role, one row per principal</CardDescription>
      </CardHeader>
      <CardContent>
        <p data-testid="principals-placeholder" className="text-sm text-muted-foreground">
          The membership surface is not frozen yet, so this node serves no principals field.
          Ticket 019 fills this panel.
        </p>
      </CardContent>
    </Card>
  );
}
