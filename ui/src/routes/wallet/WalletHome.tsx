import { Link } from "react-router";

import { listIdentities } from "@/api/client";
import { DeclaredKindNote, DeclaredKindValue } from "@/components/DeclaredKind";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { useResource } from "@/hooks/useResource";

import { IdentityCreateForm } from "./IdentityCreateForm";
import { NodeInfoPanel } from "./NodeInfoPanel";

export function WalletHome() {
  const identities = useResource(listIdentities, []);

  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <NodeInfoPanel />
      <IdentityCreateForm onCreated={identities.reload} />
      <Card className="lg:col-span-2" data-testid="identity-list">
        <CardHeader>
          <CardTitle>Identities</CardTitle>
          <DeclaredKindNote testId="identity-list-declared-kind-note" />
        </CardHeader>
        <CardContent>
          {identities.loading && <p data-testid="identity-list-loading">loading</p>}
          {identities.error && (
            <ErrorEnvelopeView error={identities.error} testId="identity-list-error" />
          )}
          {identities.data && identities.data.identities.length === 0 && (
            <p data-testid="identity-list-empty">no identities in this node home</p>
          )}
          {identities.data && identities.data.identities.length > 0 && (
            <Table stack="md">
              <TableHeader>
                <TableRow>
                  <TableHead>alias</TableHead>
                  <TableHead>declared_kind</TableHead>
                  <TableHead>identity_id</TableHead>
                  <TableHead>head_seq</TableHead>
                  <TableHead>event_count</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {identities.data.identities.map((identity) => (
                  <TableRow
                    key={identity.identity_id}
                    data-testid={`identity-row-${identity.identity_id}`}
                  >
                    <TableCell label="alias">
                      <Link
                        to={`/wallet/identities/${identity.identity_id}`}
                        className="text-sm underline"
                        data-testid={`identity-link-${identity.identity_id}`}
                      >
                        {identity.alias}
                      </Link>
                    </TableCell>
                    <TableCell label="declared_kind">
                      <DeclaredKindValue
                        kind={identity.declared_kind}
                        testId={`identity-declared-kind-${identity.identity_id}`}
                      />
                    </TableCell>
                    <TableCell label="identity_id">
                      <Identifier value={identity.identity_id} />
                    </TableCell>
                    <TableCell
                      label="head_seq"
                      data-testid={`identity-head-seq-${identity.identity_id}`}
                    >
                      {identity.head_seq}
                    </TableCell>
                    <TableCell
                      label="event_count"
                      data-testid={`identity-event-count-${identity.identity_id}`}
                    >
                      {identity.event_count}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          )}
        </CardContent>
      </Card>
    </div>
  );
}
