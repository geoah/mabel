import { DeveloperOnly } from "@/components/DeveloperMode";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { ResolvedIdentity, ResolvedIdentityScope } from "@/components/ResolvedIdentity";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { formatTimestamp } from "@/lib/time";

import {
  GraphStalenessBanner,
  GraphSyncButton,
  GraphSyncNotices,
  useGraphSync,
} from "./GraphSyncControl";

/**
 * The crawl generation this home holds: when it ran, how far it reached, what
 * stopped it, and which identities it started from. Every local identity is a
 * root at depth 0, and the crawl walked out to `depth` from each of them
 * (proposal 003 section 3).
 */
export function GraphStatusPanel() {
  const sync = useGraphSync();
  const graph = sync.graph;

  return (
    <Card data-testid="graph-panel">
      <CardHeader>
        <CardTitle>Trust graph</CardTitle>
        <CardDescription>
          One crawl generation, replaced whole by a sync; nothing here refreshes on its own
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {graph === null ? (
          <div className="space-y-2">
            <p data-testid="graph-empty" className="text-sm">
              no crawl has run in this node home, so no lookup can answer yet
            </p>
            <GraphSyncButton sync={sync} testId="graph-panel-sync" />
          </div>
        ) : (
          <>
            <GraphStalenessBanner
              stale={graph.stale}
              lastSyncMs={graph.last_sync_ms}
              sync={sync}
              testId="graph-stale-banner"
            />
            <KeyValueTable>
              <KeyValue label="last synced" testId="graph-last-sync">
                {formatTimestamp(graph.last_sync_ms)}
              </KeyValue>
              <KeyValue label="identities reached" testId="graph-node-count">
                {graph.node_count}
              </KeyValue>
              <KeyValue label="attestations read" testId="graph-edge-count">
                {graph.edge_count}
              </KeyValue>
              <KeyValue label="depth crawled" testId="graph-panel-depth">
                {graph.depth}
              </KeyValue>
            </KeyValueTable>
            {graph.truncated && (
              <details data-testid="graph-truncation" className="rounded-md border">
                <summary className="flex min-h-11 cursor-pointer list-none items-center px-3 py-2 text-sm marker:content-none hover:bg-accent">
                  <span>
                    this crawl stopped early, truncated by{" "}
                    <span data-testid="graph-truncated-by-name" className="font-mono text-xs">
                      {graph.truncated_by}
                    </span>
                  </span>
                </summary>
                <p className="border-t px-3 py-2 text-sm">
                  Caps come first and completeness never: the crawl stops at its depth, node,
                  fetch or time cap and reports which one it hit. An identity missing from this
                  generation was not reached, which is not a statement that it does not exist.
                </p>
              </details>
            )}
            <div className="space-y-1">
              <p className="text-xs text-muted-foreground">
                crawl roots, each a local identity at depth 0
              </p>
              <ResolvedIdentityScope identities={graph.roots}>
                <ul data-testid="graph-roots" className="divide-y">
                  {graph.roots.map((root) => (
                    <li
                      key={root.identity_id}
                      data-testid={`graph-root-${root.identity_id}`}
                      className="flex flex-wrap items-center gap-x-2 gap-y-1 py-2"
                    >
                      <ResolvedIdentity
                        identity={root}
                        testId={`graph-root-name-${root.identity_id}`}
                        to={`/wallet/lookup/${root.identity_id}`}
                      />
                      <span className="text-xs text-muted-foreground">depth 0</span>
                    </li>
                  ))}
                </ul>
              </ResolvedIdentityScope>
            </div>
            {graph.equivocations.length > 0 && (
              <div className="space-y-1">
                <p data-testid="graph-equivocation-count" className="text-sm">
                  {graph.equivocations.length} identity
                  {graph.equivocations.length === 1 ? "" : "s"} in this crawl served two signed
                  events at one sequence
                </p>
                <ul className="space-y-1">
                  {graph.equivocations.map((identityId) => (
                    <li key={identityId} data-testid={`graph-equivocation-${identityId}`}>
                      <Identifier value={identityId} />
                    </li>
                  ))}
                </ul>
              </div>
            )}
            <div className="flex flex-wrap items-center gap-2">
              <GraphSyncButton sync={sync} testId="graph-panel-sync" />
              <DeveloperOnly>
                <span data-testid="graph-panel-sync-id" className="text-xs text-muted-foreground">
                  sync_id <Identifier value={graph.sync_id} full />
                </span>
              </DeveloperOnly>
            </div>
          </>
        )}
        <GraphSyncNotices sync={sync} />
      </CardContent>
    </Card>
  );
}
