import type { Equivocation, LookupPath, LookupResponse } from "@/api/types";
import { Identifier } from "@/components/Identifier";
import { InfoTip } from "@/components/InfoTip";
import {
  type CardTestIds,
  factsFromResolved,
  IdentityCard,
  type IdentityCardEntry,
  IdentityCardList,
  IdentityInline,
  IdentityPillBadge,
  usePill,
} from "@/components/identity";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleChevron,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { describeAge } from "@/lib/time";
import { GraphStalenessBanner, useGraphSync } from "@/routes/wallet/GraphSyncControl";

/**
 * The reverse list is never "who trusts them". It is who your wallet happens to
 * have seen trusting them, and the info icon beside the list says so.
 */
export const REVERSE_LABEL =
  "Best effort: who your wallet has seen trusting them, not everyone who does";

/** Two signed entries at one position, drawn wherever the crawl recorded one. */
export function EquivocationNotice({
  equivocation,
  testId,
}: {
  equivocation: Equivocation;
  testId: string;
}) {
  return (
    <div
      data-testid={testId}
      className="w-full space-y-1 rounded-md border border-destructive p-2 text-xs"
    >
      <p>
        Two different entries were signed at position{" "}
        <span data-testid={`${testId}-seq`}>{equivocation.at_seq}</span>. Your wallet cannot tell
        which one is the real record.
      </p>
      <ul className="space-y-1">
        {equivocation.branches.map((branch) => (
          <li key={branch.event} data-testid={`${testId}-branch-${branch.event}`}>
            <Identifier value={branch.event} />
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The verdict, in one sentence, with the pill that says the same thing shorter. */
function Verdict({ identityId, degrees }: { identityId: string; degrees: number | null }) {
  const pill = usePill(identityId);
  return (
    <div className="flex flex-wrap items-center gap-2">
      {/* The pill is the sentence in two words, so it is drawn beside the one it
          agrees with: a pill reading trusted next to "no connection found" is
          two answers to one question. */}
      {pill !== null && degrees !== null && (
        <IdentityPillBadge pill={pill} testId="lookup-verdict-pill" />
      )}
      {degrees === null ? (
        // Both testids are kept: the sentence, and the verdict inside it.
        <p data-testid="lookup-degrees-none" className="text-sm">
          <span data-testid="lookup-degrees">No connection found</span> yet.
        </p>
      ) : (
        <p data-testid="lookup-degrees" className="text-sm">
          {degrees === 1 ? "You trust them directly" : `Connected through ${degrees} steps`}
        </p>
      )}
    </div>
  );
}

function ArrowDown() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" className="size-3.5 shrink-0" fill="currentColor">
      <path d="M8 2.5a.75.75 0 0 1 .75.75v7.19l2.72-2.72a.75.75 0 1 1 1.06 1.06l-4 4a.75.75 0 0 1-1.06 0l-4-4a.75.75 0 1 1 1.06-1.06l2.72 2.72V3.25A.75.75 0 0 1 8 2.5Z" />
    </svg>
  );
}

/**
 * The card testids of one step of a path. A hop's card is `lookup-hop-i-j` and
 * the identity on it is `lookup-hop-i-j-to`, which is what the suites read; the
 * first card of a chain is the root the answer came from.
 */
function hopTestIds(index: number, hopIndex: number): CardTestIds {
  const base = `lookup-hop-${index}-${hopIndex}`;
  return (part) => (part === "" ? base : part === "name" ? `${base}-to` : `${base}-${part}`);
}

function rootTestIds(index: number): CardTestIds {
  const base = `lookup-path-${index}-root`;
  return (part) =>
    part === "" ? base : part === "name" ? `lookup-hop-${index}-0-from` : `${base}-${part}`;
}

/**
 * One path, as a chain of the same identity cards every other screen draws: the
 * root you asked from, then one card per step, each under the word that links
 * them. Who trusts whom is the order, top to bottom.
 */
function PathChain({ path, index }: { path: LookupPath; index: number }) {
  const root = path.hops[0]?.from;
  if (root === undefined) {
    return null;
  }
  return (
    <ol data-testid={`lookup-path-${index}`} className="space-y-1">
      <li className="min-w-0">
        <IdentityCard
          facts={factsFromResolved(root, { to: `/identities/${root.identity_id}` })}
          testIds={rootTestIds(index)}
        />
      </li>
      {path.hops.map((hop, hopIndex) => (
        <li key={hop.attestation_event} className="min-w-0 space-y-1">
          <p className="flex items-center gap-1 pl-3 text-xs text-muted-foreground">
            <ArrowDown />
            trusts
          </p>
          <IdentityCard
            facts={factsFromResolved(hop.to, {
              stale: hop.stale,
              to: `/identities/${hop.to.identity_id}`,
            })}
            testIds={hopTestIds(index, hopIndex)}
            markers={
              <>
                <span data-testid={`lookup-hop-${index}-${hopIndex}-fetched`}>
                  seen {describeAge(hop.fetched_at_ms)}
                </span>
                {hop.stale && (
                  <span data-testid={`lookup-hop-${index}-${hopIndex}-stale`} className="italic">
                    may be out of date
                  </span>
                )}
              </>
            }
          />
          {hop.equivocation && (
            <EquivocationNotice
              equivocation={hop.equivocation}
              testId={`lookup-hop-${index}-${hopIndex}-equivocation`}
            />
          )}
        </li>
      ))}
    </ol>
  );
}

/** One trust or reverse list, folded away behind its own count. */
function EntryList({
  title,
  count,
  testId,
  entries,
  empty,
  info,
}: {
  title: string;
  count: number;
  /** `lookup-trust` or `lookup-reverse`: the list, its toggle and its empty line. */
  testId: string;
  entries: IdentityCardEntry[];
  empty: string;
  info?: string;
}) {
  return (
    <Collapsible className="rounded-md border">
      <CollapsibleTrigger
        data-testid={`${testId}-toggle`}
        className="flex min-h-11 w-full items-center gap-2 px-3 text-left text-sm hover:bg-accent"
      >
        <CollapsibleChevron />
        <span data-testid={`${testId}-label`}>{title}</span>
        {info !== undefined && <InfoTip text={info} testId={`${testId}-note`} />}
        <span className="ml-auto text-xs text-muted-foreground">{count}</span>
      </CollapsibleTrigger>
      <CollapsibleContent className="border-t p-3">
        <IdentityCardList
          entries={entries}
          testId={testId}
          empty={empty}
          emptyTestId={`${testId}-empty`}
        />
      </CollapsibleContent>
    </Collapsible>
  );
}

/**
 * How you know them: the verdict, the path that reached them, and the two lists
 * the crawl holds. Every identity here is an identity card, so a name on a path
 * reads exactly as it does in the wallet.
 */
export function KnowledgeSection({
  response,
  onSynced,
}: {
  response: LookupResponse;
  /** Reloads the lookup this section drew, once a sync has replaced the crawl. */
  onSynced: () => void;
}) {
  const sync = useGraphSync(onSynced);
  // The paths already carry the equivocation of every node they reach, so the
  // heading only warns about one no hop is going to show.
  const onAPath = response.paths.some((path) =>
    path.hops.some(
      (hop) => hop.equivocation !== null && hop.to.identity_id === response.identity.identity_id,
    ),
  );
  const trust: IdentityCardEntry[] = response.trust.map((entry) => ({
    facts: factsFromResolved(entry.subject, {
      to: `/identities/${entry.subject.identity_id}`,
    }),
  }));
  const reverse: IdentityCardEntry[] = response.reverse.entries.map((entry) => ({
    facts: factsFromResolved(entry.identity, {
      to: `/identities/${entry.identity.identity_id}`,
    }),
  }));

  return (
    <Card data-testid="lookup-result">
      <CardHeader>
        <CardTitle>How you know them</CardTitle>
        <CardDescription className="flex flex-wrap items-baseline gap-2">
          from your
          <IdentityInline identity={response.from} testId="lookup-from" />
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        <GraphStalenessBanner
          stale={response.graph_stale}
          lastSyncMs={response.last_sync_ms}
          sync={sync}
          testId="lookup-graph-stale"
        />
        {response.graph_truncated && (
          <p data-testid="lookup-graph-truncated" className="text-sm">
            Your wallet may not have seen everything.
          </p>
        )}
        {response.equivocation && !onAPath && (
          <EquivocationNotice equivocation={response.equivocation} testId="lookup-equivocation" />
        )}
        <Verdict identityId={response.identity.identity_id} degrees={response.degrees} />
        {response.paths.length > 0 && (
          <div data-testid="lookup-paths" className="space-y-3">
            {response.paths.map((path, index) => (
              <PathChain
                key={path.hops.map((hop) => hop.attestation_event).join("-")}
                path={path}
                index={index}
              />
            ))}
          </div>
        )}
        <EntryList
          title="Who they trust"
          count={trust.length}
          testId="lookup-trust"
          entries={trust}
          empty="Your wallet has not seen them trust anyone."
        />
        <EntryList
          title="Who your wallet has seen trusting them"
          count={reverse.length}
          testId="lookup-reverse"
          entries={reverse}
          empty="Your wallet has not seen anyone trust them."
          info={REVERSE_LABEL}
        />
      </CardContent>
    </Card>
  );
}
