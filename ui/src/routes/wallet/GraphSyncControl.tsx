import { type ReactNode, useState } from "react";

import { type ApiError, getGraph, syncGraph } from "@/api/client";
import type { Graph } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { asApiError, useResource } from "@/hooks/useResource";
import { GRAPH_CONSENT_KEY, useConsent } from "@/lib/preferences";
import { describeAge } from "@/lib/time";

/**
 * What a sync tells the world, stated before the first one and remembered per
 * node home (proposal 003, Consequences).
 */
const GRAPH_CONSENT_SENTENCES = [
  "Every witness your wallet asks learns which people you are interested in.",
  "Your wallet reads their records to answer how you know someone, and keeps no copy.",
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
export function useGraphSync(
  /**
   * Called once a sync has replaced the crawl. Every answer a screen already
   * read came from the old generation, so the screen that asked for the sync
   * reloads what it drew: a fresh crawl behind a stale answer is the staleness
   * banner all over again.
   */
  onSynced?: () => void,
): GraphSync {
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
      onSynced?.();
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
              Look now
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
    <Button
      variant="outline"
      size="sm"
      data-testid={testId}
      disabled={sync.pending}
      onClick={sync.start}
    >
      {sync.pending ? "looking" : "Look again"}
    </Button>
  );
}

/**
 * The one place a sync starts from, on the witnesses page: the wallet learns
 * about people by reading what witnesses hold, so the button lives with the
 * witnesses. Nothing is automatic and there is no timer (proposal 003 section
 * 3), so the card says when the last look happened and offers the next one.
 */
export function GraphSyncCard() {
  const sync = useGraphSync();
  const current = sync.graph;

  return (
    <Card data-testid="graph-sync">
      <CardHeader>
        <CardTitle>Finding people through the people you trust</CardTitle>
        <CardDescription>
          Your wallet follows who trusts whom, and only when you press the button.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-2">
        <p data-testid="graph-sync-state" className="text-sm">
          {current === null
            ? "Your wallet has not looked yet."
            : `Your wallet last looked ${describeAge(current.last_sync_ms)}.`}
        </p>
        {current?.truncated && (
          <p data-testid="graph-sync-truncated" className="text-sm">
            Your wallet may not have seen everything.
          </p>
        )}
        <GraphSyncButton sync={sync} testId="graph-sync-button" />
        <GraphSyncNotices sync={sync} />
      </CardContent>
    </Card>
  );
}

/**
 * The banner an aged answer raises, wherever a screen reads the graph. Nothing
 * refreshes it on its own, so the banner carries the button (proposal 003
 * section 3).
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
        Your wallet last looked{" "}
        {lastSyncMs === null ? "never" : describeAge(lastSyncMs)}. Look again for a fresher answer.
      </span>
      <GraphSyncButton sync={sync} testId={`${testId}-sync`} />
      {/* A sync started here asks for the same consent as one started from the
          card, so the panel travels with the button rather than the card. */}
      {(sync.asking || sync.error !== null) && (
        <div className="w-full">
          <GraphSyncNotices sync={sync} />
        </div>
      )}
    </div>
  );
}

export { GraphSyncNotices };
