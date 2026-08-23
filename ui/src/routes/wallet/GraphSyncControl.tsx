import { useState } from "react";

import { type ApiError, getGraph, syncGraph } from "@/api/client";
import type { Graph } from "@/api/types";
import { DeveloperOnly } from "@/components/DeveloperMode";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Button } from "@/components/ui/button";
import { asApiError, useResource } from "@/hooks/useResource";
import { GRAPH_CONSENT_KEY, useConsent } from "@/lib/preferences";

/**
 * What a sync tells the world, stated before the first one and remembered per
 * node home (proposal 003, Consequences).
 */
const GRAPH_CONSENT_SENTENCES = [
  "A graph sync tells each contacted witness which identities this wallet cares about.",
  "It fetches ledgers this home does not hold, and keeps them in a crawl generation, not as replicas.",
];

/** The header control: the counts of the current crawl, and one manual sync. */
export function GraphSyncControl() {
  const graph = useResource(getGraph, []);
  const [consented, giveConsent] = useConsent(GRAPH_CONSENT_KEY);
  const [asking, setAsking] = useState(false);
  const [pending, setPending] = useState(false);
  const [synced, setSynced] = useState<Graph | null>(null);
  const [error, setError] = useState<ApiError | null>(null);
  const current = synced ?? graph.data?.graph ?? null;

  async function run() {
    setPending(true);
    setError(null);
    try {
      const response = await syncGraph();
      setSynced(response.graph);
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  function start() {
    if (!consented) {
      setAsking(true);
      return;
    }
    void run();
  }

  function confirm() {
    giveConsent();
    setAsking(false);
    void run();
  }

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
        <Button
          variant="outline"
          size="sm"
          data-testid="graph-sync-button"
          disabled={pending}
          onClick={start}
        >
          {pending ? "synchronizing" : "Sync graph"}
        </Button>
      </div>
      {(asking || error) && (
        <div className="absolute right-0 top-full z-30 mt-2 w-80 space-y-2 rounded-md border bg-card p-3 text-left shadow-md">
          {asking && (
            <div data-testid="graph-sync-consent" className="space-y-2">
              {GRAPH_CONSENT_SENTENCES.map((sentence) => (
                <p key={sentence} className="text-xs">
                  {sentence}
                </p>
              ))}
              <div className="flex gap-2">
                <Button size="sm" data-testid="graph-sync-consent-confirm" onClick={confirm}>
                  Synchronize
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  data-testid="graph-sync-consent-cancel"
                  onClick={() => setAsking(false)}
                >
                  Cancel
                </Button>
              </div>
            </div>
          )}
          {error && <ErrorEnvelopeView error={error} testId="graph-sync-error" />}
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
