import { getNode } from "@/api/client";
import type { WalletNodeInfo, WitnessNodeInfo } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { WitnessCard } from "@/components/WitnessCard";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

/** The relay setting, in the two words it is worth. */
const RELAY: Record<string, string> = {
  n0: "public relays",
  disabled: "direct connections only",
};

/** Bytes as a person reads them: three significant figures and a unit. */
function bytes(value: number): string {
  const units = ["bytes", "kB", "MB", "GB", "TB"];
  let scaled = value;
  let unit = 0;
  while (scaled >= 1000 && unit < units.length - 1) {
    scaled /= 1000;
    unit += 1;
  }
  const rounded = unit === 0 ? scaled : Math.round(scaled * 10) / 10;
  return `${rounded} ${units[unit]}`;
}

/** The counts a wallet adds, which are about the identities it holds. */
function WalletRows({ node }: { node: WalletNodeInfo }) {
  return (
    <KeyValue label="identities" testId="node-identity-count">
      {node.identity_count}
    </KeyValue>
  );
}

/** The counts a witness adds, which are about the records it keeps for others. */
function WitnessRows({ node }: { node: WitnessNodeInfo }) {
  return (
    <>
      <KeyValue label="records" testId="node-ledger-count">
        {node.ledger_count}
      </KeyValue>
      <KeyValue label="conflicts" testId="node-fork-count">
        {node.fork_count}
      </KeyValue>
    </>
  );
}

/**
 * This node, as short rows: what it is, the Iroh ID other nodes dial it by, how
 * it is reachable, what it holds and which build is running. Everything here is
 * `GET /api/node` and nothing else.
 */
export function NodePage() {
  const node = useResource(getNode, []);
  const data = node.data;

  return (
    <div className="space-y-4">
      <Card data-testid="node-page">
        <CardHeader>
          <CardTitle>This node</CardTitle>
        </CardHeader>
        <CardContent>
          {node.loading && <p data-testid="node-loading">loading</p>}
          {node.error && <ErrorEnvelopeView error={node.error} testId="node-error" />}
          {data && (
            <KeyValueTable>
              <KeyValue label="role" testId="node-role">
                {data.role}
              </KeyValue>
              <KeyValue label="Iroh ID" testId="node-endpoint-id">
                <Identifier value={data.endpoint_id} full />
              </KeyValue>
              <KeyValue label="relay" testId="node-relay">
                {RELAY[data.relay] ?? data.relay}
              </KeyValue>
              {data.role === "wallet" ? (
                <WalletRows node={data} />
              ) : (
                <WitnessRows node={data} />
              )}
              <KeyValue label="space used" testId="node-storage">
                {bytes(data.storage_used)} of {bytes(data.storage_capacity)}
              </KeyValue>
              <KeyValue label="version" testId="node-version">
                {data.version}
              </KeyValue>
            </KeyValueTable>
          )}
        </CardContent>
      </Card>
      {data && (
        <Card data-testid="node-witnesses">
          <CardHeader>
            <CardTitle>Witnesses it uses by default</CardTitle>
          </CardHeader>
          <CardContent>
            {data.witnesses.length === 0 ? (
              <p data-testid="node-witnesses-empty" className="text-sm">
                none
              </p>
            ) : (
              <ul data-testid="node-witness-cards" className="grid gap-2">
                {data.witnesses.map((endpointId) => (
                  <li key={endpointId} className="min-w-0">
                    <WitnessCard endpointId={endpointId} testIdPrefix="node-witness" />
                  </li>
                ))}
              </ul>
            )}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
