import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

/**
 * Ticket 014 builds the witness route: node info, the ledger table, one ledger
 * with its events, and the fork list. This placeholder holds the route so the
 * shell ships one bundle covering both.
 */
export function WitnessHome() {
  return (
    <Card data-testid="witness-placeholder">
      <CardHeader>
        <CardTitle>Witness</CardTitle>
        <CardDescription>Read-only diagnostics for the ledgers this node holds</CardDescription>
      </CardHeader>
      <CardContent>
        <p className="text-sm text-muted-foreground" data-testid="witness-placeholder-note">
          Ticket 014 fills this route with node info, the ledger list, one ledger with its
          events, and the fork list.
        </p>
      </CardContent>
    </Card>
  );
}
