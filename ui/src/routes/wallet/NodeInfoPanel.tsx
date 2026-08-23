import { getWalletNode } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Field, FieldGrid } from "@/components/Field";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

export function NodeInfoPanel() {
  const node = useResource(getWalletNode, []);

  return (
    <Card data-testid="node-info">
      <CardHeader>
        <CardTitle>Node</CardTitle>
      </CardHeader>
      <CardContent>
        {node.loading && <p data-testid="node-info-loading">loading</p>}
        {node.error && <ErrorEnvelopeView error={node.error} testId="node-info-error" />}
        {node.data && (
          <FieldGrid>
            <Field label="role" testId="node-role">
              {node.data.role}
            </Field>
            <Field label="endpoint_id" testId="node-endpoint-id" mono>
              {node.data.endpoint_id}
            </Field>
            <Field label="http_bind" testId="node-http-bind">
              {node.data.http_bind}
            </Field>
            <Field label="relay" testId="node-relay">
              {node.data.relay}
            </Field>
            <Field label="witnesses" testId="node-witnesses" mono>
              {node.data.witnesses.length === 0 ? "none" : node.data.witnesses.join(", ")}
            </Field>
            <Field label="storage_capacity" testId="node-storage-capacity">
              {node.data.storage_capacity}
            </Field>
            <Field label="storage_used" testId="node-storage-used">
              {node.data.storage_used}
            </Field>
            <Field label="identity_count" testId="node-identity-count">
              {node.data.identity_count}
            </Field>
            <Field label="version" testId="node-version">
              {node.data.version}
            </Field>
          </FieldGrid>
        )}
      </CardContent>
    </Card>
  );
}
