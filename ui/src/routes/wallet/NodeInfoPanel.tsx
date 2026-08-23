import { getWalletNode } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Field, FieldGrid } from "@/components/Field";
import { Identifier } from "@/components/Identifier";
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
            <Field label="endpoint_id" testId="node-endpoint-id">
              <Identifier value={node.data.endpoint_id} />
            </Field>
            <Field label="http_bind" testId="node-http-bind" mono>
              {node.data.http_bind}
            </Field>
            <Field label="relay" testId="node-relay">
              {node.data.relay}
            </Field>
            <Field label="witnesses" testId="node-witnesses">
              {node.data.witnesses.length === 0 ? (
                "none"
              ) : (
                <span className="flex flex-col gap-1">
                  {node.data.witnesses.map((witness) => (
                    <Identifier key={witness} value={witness} />
                  ))}
                </span>
              )}
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
