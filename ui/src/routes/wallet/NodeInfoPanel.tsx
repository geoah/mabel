import { getWalletNode } from "@/api/client";
import { DeveloperOnly } from "@/components/DeveloperMode";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

/**
 * What this node is. The endpoint id, the binds and the witness endpoint ids
 * are developer detail (decision 014), so they wait behind the toggle; the role
 * and the identity count answer "whose wallet is this" and stay.
 */
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
          <KeyValueTable>
            <KeyValue label="role" testId="node-role">
              {node.data.role}
            </KeyValue>
            <KeyValue label="identity_count" testId="node-identity-count">
              {node.data.identity_count}
            </KeyValue>
            <DeveloperOnly>
              <KeyValue label="endpoint_id" testId="node-endpoint-id">
                <Identifier value={node.data.endpoint_id} />
              </KeyValue>
              <KeyValue label="http_bind" testId="node-http-bind">
                <span className="font-mono text-xs">{node.data.http_bind}</span>
              </KeyValue>
              <KeyValue label="relay" testId="node-relay">
                {node.data.relay}
              </KeyValue>
              <KeyValue label="witnesses" testId="node-witnesses">
                {node.data.witnesses.length === 0 ? (
                  "none"
                ) : (
                  <span className="flex flex-col gap-1">
                    {node.data.witnesses.map((witness) => (
                      <Identifier key={witness} value={witness} />
                    ))}
                  </span>
                )}
              </KeyValue>
              <KeyValue label="storage_capacity" testId="node-storage-capacity">
                {node.data.storage_capacity}
              </KeyValue>
              <KeyValue label="storage_used" testId="node-storage-used">
                {node.data.storage_used}
              </KeyValue>
              <KeyValue label="version" testId="node-version">
                {node.data.version}
              </KeyValue>
            </DeveloperOnly>
          </KeyValueTable>
        )}
      </CardContent>
    </Card>
  );
}
