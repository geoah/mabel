import { type ReactNode, useState } from "react";

import { type ApiError, getGraph, syncGraph } from "@/api/client";
import type { Graph } from "@/api/types";
import { DeveloperOnly } from "@/components/DeveloperMode";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Button } from "@/components/ui/button";
import { asApiError, useResource } from "@/hooks/useResource";
import { GRAPH_CONSENT_KEY, useConsent } from "@/lib/preferences";
import { describeAge, formatTimestamp } from "@/lib/time";

/**
 * What a sync tells the world, stated before the first one and remembered per
 * node home (proposal 003, Consequences).
 */
const GRAPH_CONSENT_SENTENCES = [
  "A graph sync tells each contacted witness which identities this wallet cares about.",
  "It fetches ledgers this home does not hold, and keeps them in a crawl generation, not as replicas.",
];

interface GraphSync {
  graph: Graph | null;
  pending: boolean;
  asking: boolean;
  error: ApiError | null;
  start: () => void;
  confirm: () => void;
  cancel: () => void;
}

/**
 * The crawl generation this home holds and the one manual sync that replaces
 * it. Synchronizing is never automatic: there is no timer anywhere (proposal
 * 003 section 3).
 */
export function useGraphSync(): GraphSync {
  const loaded = useResource(getGraph, []);
  const [consented, giveConsent] = useConsent(GRAPH_CONSENT_KEY);
  const [asking, setAsking] = useState(false);
  const [pending, setPending] = useState(false);
  const [synced, setSynced] = useState<Graph | null>(null);
  const [error, setError] = useState<ApiError | null>(null);

  async function run() {
    setPending(true);
    setError(null);
    try {
      setSynced((await syncGraph()).graph);
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return {
    graph: synced ?? loaded.data?.graph ?? null,
    pending,
    asking,
    error,
    start: () => {
      if (!consented) {
        setAsking(true);
        return;
      }
      void run();
    },
    confirm: () => {
      giveConsent();
      setAsking(false);
      void run();
    },
    cancel: () => setAsking(false),
  };
}

/** The consent panel and the error envelope a sync can raise, in one block. */
function GraphSyncNotices({ sync, children }: { sync: GraphSync; children?: ReactNode }) {
  if (!sync.asking && sync.error === null) {
    return null;
  }
  return (
    <div className="space-y-2 rounded-md border bg-card p-3 text-left">
      {sync.asking && (
        <div data-testid="graph-sync-consent" className="space-y-2">
          {GRAPH_CONSENT_SENTENCES.map((sentence) => (
            <p key={sentence} className="text-xs">
              {sentence}
            </p>
          ))}
          <div className="flex gap-2">
            <Button size="sm" data-testid="graph-sync-consent-confirm" onClick={sync.confirm}>
              Synchronize
            </Button>
            <Button
              size="sm"
              variant="outline"
              data-testid="graph-sync-consent-cancel"
              onClick={sync.cancel}
            >
              Cancel
            </Button>
          </div>
        </div>
      )}
      {sync.error && <ErrorEnvelopeView error={sync.error} testId="graph-sync-error" />}
      {children}
    </div>
  );
}

export function GraphSyncButton({ sync, testId }: { sync: GraphSync; testId: string }) {
  return (
    <Button variant="outline" size="sm" data-testid={testId} disabled={sync.pending} onClick={sync.start}>
      {sync.pending ? "synchronizing" : "Sync graph"}
    </Button>
  );
}

/** The header control: the counts of the current crawl, and one manual sync. */
export function GraphSyncControl() {
  const sync = useGraphSync();
  const current = sync.graph;

  return (
    <div className="relative" data-testid="graph-sync">
      <div className="flex items-center gap-2">
        {current && (
          <span
            data-testid="graph-sync-counts"
            className="hidden text-xs text-muted-foreground sm:inline"
          >
            {current.node_count} identities, {current.edge_count} attestations
          </span>
        )}
        {current?.truncated && (
          <span data-testid="graph-sync-truncated" className="hidden text-xs sm:inline">
            truncated by {current.truncated_by}
          </span>
        )}
        <GraphSyncButton sync={sync} testId="graph-sync-button" />
      </div>
      {(sync.asking || sync.error) && (
        <div className="absolute right-0 top-full z-30 mt-2 w-80">
          <GraphSyncNotices sync={sync} />
        </div>
      )}
      {current && (
        <DeveloperOnly>
          {/* Crawl provenance and sync freshness, in the flow so it hides nothing. */}
          <div className="mt-2 w-72 rounded-md border bg-card p-2 text-left">
            <KeyValueTable data-testid="graph-sync-provenance">
              <KeyValue label="sync_id" testId="graph-sync-id">
                <Identifier value={current.sync_id} full />
              </KeyValue>
              <KeyValue label="last_sync_ms" testId="graph-last-sync-ms">
                {current.last_sync_ms}
              </KeyValue>
              <KeyValue label="depth" testId="graph-depth">
                {current.depth}
              </KeyValue>
              <KeyValue label="fetch_count" testId="graph-fetch-count">
                {current.fetch_count}
              </KeyValue>
              <KeyValue label="truncated_by" testId="graph-truncated-by">
                {current.truncated_by ?? "null"}
              </KeyValue>
              <KeyValue label="stale" testId="graph-stale">
                {String(current.stale)}
              </KeyValue>
              <KeyValue label="equivocations" testId="graph-equivocations">
                {current.equivocations.length}
              </KeyValue>
            </KeyValueTable>
          </div>
        </DeveloperOnly>
      )}
    </div>
  );
}

/**
 * The banner a stale crawl raises, wherever a screen reads the graph. A crawl
 * is stale 24 hours after its last sync; nothing refreshes it on its own, so
 * the banner carries the button (proposal 003 section 3).
 */
export function GraphStalenessBanner({
  stale,
  lastSyncMs,
  sync,
  testId,
}: {
  stale: boolean;
  lastSyncMs: number | null;
  sync: GraphSync;
  testId: string;
}) {
  if (!stale) {
    return null;
  }
  return (
    <div
      data-testid={testId}
      className="flex flex-wrap items-center gap-2 rounded-md border border-destructive p-2 text-sm"
    >
      <span>
        graph is stale, last synced{" "}
        {lastSyncMs === null ? "never" : `${describeAge(lastSyncMs)}, ${formatTimestamp(lastSyncMs)}`}
      </span>
      <GraphSyncButton sync={sync} testId={`${testId}-sync`} />
    </div>
  );
}

export { GraphSyncNotices };
