import { getNode } from "@/api/client";
import type { WalletNodeInfo, WitnessNodeInfo } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { useResource } from "@/hooks/useResource";

/** What the relay setting means for reaching this node. */
const RELAY_SENTENCE: Record<string, string> = {
  n0: "through the public relays, so it can be reached from behind a home router",
  disabled: "direct connections only, with no relay",
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

/** The rows a wallet adds, which are about the identities it holds. */
function WalletRows({ node }: { node: WalletNodeInfo }) {
  return (
    <KeyValue label="identities here" testId="node-identity-count">
      {node.identity_count} {node.identity_count === 1 ? "identity" : "identities"}
    </KeyValue>
  );
}

/** The rows a witness adds, which are about the records it keeps for others. */
function WitnessRows({ node }: { node: WitnessNodeInfo }) {
  return (
    <>
      <KeyValue label="records kept here" testId="node-ledger-count">
        {node.ledger_count} {node.ledger_count === 1 ? "record" : "records"}
      </KeyValue>
      <KeyValue label="conflicts recorded" testId="node-fork-count">
        {node.fork_count}
      </KeyValue>
    </>
  );
}

/**
 * This node, in the words of the one route that describes it: what it is for,
 * the id other nodes dial it by, how it is reachable, where its HTTP API
 * listens, what it holds and which build is running. Everything on this page is
 * `GET /api/node` and nothing else, so there is no connection ticket here: the
 * API serves none.
 */
export function NodePage() {
  const node = useResource(getNode, []);
  const data = node.data;

  return (
    <Card data-testid="node-page">
      <CardHeader>
        <CardTitle>This node</CardTitle>
        <CardDescription>
          The program on this computer that keeps your records and talks to other people&apos;s
          nodes. Other nodes reach it by the id below.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {node.loading && <p data-testid="node-loading">loading</p>}
        {node.error && <ErrorEnvelopeView error={node.error} testId="node-error" />}
        {data && (
          <KeyValueTable>
            <KeyValue label="what it does" testId="node-role">
              {data.role === "wallet"
                ? "holds your identities and signs for them"
                : "keeps copies of other people's records"}
            </KeyValue>
            <KeyValue label="its id" testId="node-endpoint-id">
              <Identifier value={data.endpoint_id} full />
            </KeyValue>
            <KeyValue label="how it is reachable" testId="node-relay">
              {RELAY_SENTENCE[data.relay] ?? data.relay}
            </KeyValue>
            <KeyValue label="where it serves this page" testId="node-http-bind">
              <span className="font-mono text-xs">{data.http_bind}</span>
            </KeyValue>
            {data.role === "wallet" ? (
              <WalletRows node={data} />
            ) : (
              <WitnessRows node={data} />
            )}
            <KeyValue label="space used" testId="node-storage">
              {bytes(data.storage_used)} of {bytes(data.storage_capacity)}
            </KeyValue>
            <KeyValue label="witnesses it uses by default" testId="node-witnesses">
              {data.witnesses.length === 0 ? (
                "none"
              ) : (
                <span className="flex flex-col gap-1">
                  {data.witnesses.map((endpointId) => (
                    <Identifier
                      key={endpointId}
                      value={endpointId}
                      full
                      to={`/witnesses/${endpointId}`}
                      linkTestId={`node-witness-link-${endpointId}`}
                    />
                  ))}
                </span>
              )}
            </KeyValue>
            <KeyValue label="version" testId="node-version">
              {data.version}
            </KeyValue>
          </KeyValueTable>
        )}
      </CardContent>
    </Card>
  );
}
