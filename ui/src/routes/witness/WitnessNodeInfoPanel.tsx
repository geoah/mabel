import { getWitnessNode } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Field, FieldGrid } from "@/components/Field";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

import { WITNESS_READ_ONLY_NOTE } from "./notes";

/** GET /api/node on a witness: the same document as the wallet plus the two counts. */
export function WitnessNodeInfoPanel() {
  const node = useResource(getWitnessNode, []);

  return (
    <Card data-testid="witness-node-info">
      <CardHeader>
        <CardTitle>Node</CardTitle>
        <CardDescription data-testid="witness-read-only-note">
          {WITNESS_READ_ONLY_NOTE}
        </CardDescription>
      </CardHeader>
      <CardContent>
        {node.loading && <p data-testid="witness-node-info-loading">loading</p>}
        {node.error && <ErrorEnvelopeView error={node.error} testId="witness-node-info-error" />}
        {node.data && (
          <FieldGrid>
            <Field label="role" testId="witness-node-role">
              {node.data.role}
            </Field>
            <Field label="endpoint_id" testId="witness-node-endpoint-id" mono>
              {node.data.endpoint_id}
            </Field>
            <Field label="http_bind" testId="witness-node-http-bind">
              {node.data.http_bind}
            </Field>
            <Field label="relay" testId="witness-node-relay">
              {node.data.relay}
            </Field>
            <Field label="storage_capacity" testId="witness-node-storage-capacity">
              {node.data.storage_capacity}
            </Field>
            <Field label="storage_used" testId="witness-node-storage-used">
              {node.data.storage_used}
            </Field>
            <Field label="ledger_count" testId="witness-node-ledger-count">
              {node.data.ledger_count}
            </Field>
            <Field label="fork_count" testId="witness-node-fork-count">
              {node.data.fork_count}
            </Field>
            <Field label="version" testId="witness-node-version">
              {node.data.version}
            </Field>
          </FieldGrid>
        )}
      </CardContent>
    </Card>
  );
}
