import { lookup } from "@/api/client";
import type {
  Equivocation,
  LookupHop,
  LookupResponse,
  ResolvedIdentity as ResolvedIdentityDocument,
} from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { IdentityInline, IdentityListScope } from "@/components/identity";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleChevron,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { useResource } from "@/hooks/useResource";
import { describeAge } from "@/lib/time";
import { GraphStalenessBanner, useGraphSync } from "@/routes/wallet/GraphSyncControl";

/**
 * How far the section expands in place. The identity page itself is level 0, so
 * a reader can open a name and open one name inside it, and no further: a walk
 * that kept expanding would render the whole crawl (proposal 003 section 4).
 */
export const MAX_LEVEL = 2;

/**
 * The reverse list is never "who trusts them". It is who your wallet happens to
 * have seen trusting them, and it says so wherever it is drawn.
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

/** One step of a chain: who trusts whom, and how fresh that reading is. */
function Hop({ hop, testId }: { hop: LookupHop; testId: string }) {
  return (
    <li data-testid={testId} className="flex flex-wrap items-center gap-x-2 gap-y-1 py-2">
      <IdentityInline identity={hop.from} testId={`${testId}-from`} />
      <span className="text-xs text-muted-foreground">trusts</span>
      <IdentityInline
        identity={hop.to}
        testId={`${testId}-to`}
        to={`/identities/${hop.to.identity_id}`}
      />
      <span data-testid={`${testId}-fetched`} className="text-xs text-muted-foreground">
        seen {describeAge(hop.fetched_at_ms)}
      </span>
      {hop.stale && (
        <span data-testid={`${testId}-stale`} className="text-xs italic">
          may be out of date
        </span>
      )}
      {hop.equivocation && (
        <EquivocationNotice equivocation={hop.equivocation} testId={`${testId}-equivocation`} />
      )}
    </li>
  );
}

/** One name in a trust or reverse list, openable while the cap allows it. */
function EntryRow({
  identity,
  from,
  level,
  kind,
}: {
  identity: ResolvedIdentityDocument;
  from: string;
  level: number;
  kind: "trust" | "reverse";
}) {
  const expandable = level < MAX_LEVEL;

  return (
    <li data-testid={`lookup-${kind}-row-${identity.identity_id}`} className="py-2">
      <Collapsible className="space-y-1">
        <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
          <IdentityInline
            identity={identity}
            testId={`lookup-${kind}-name-${identity.identity_id}`}
            to={`/identities/${identity.identity_id}`}
          />
          {expandable ? (
            <CollapsibleTrigger
              data-testid={`lookup-${kind}-expand-${identity.identity_id}`}
              className="ml-auto flex min-h-9 items-center gap-1.5 text-xs text-muted-foreground hover:text-foreground"
            >
              <CollapsibleChevron />
              <span>How you know them</span>
            </CollapsibleTrigger>
          ) : (
            <span
              data-testid={`lookup-${kind}-expand-limit-${identity.identity_id}`}
              className="ml-auto text-xs text-muted-foreground"
            >
              Open their own page to go further.
            </span>
          )}
        </div>
        {expandable && (
          <CollapsibleContent>
            <Expansion
              identityId={identity.identity_id}
              from={from}
              level={level + 1}
              kind={kind}
            />
          </CollapsibleContent>
        )}
      </Collapsible>
    </li>
  );
}

/** One nested lookup, drawn in place under the name that opened it. */
function Expansion({
  identityId,
  from,
  level,
  kind,
}: {
  identityId: string;
  from: string;
  level: number;
  kind: "trust" | "reverse";
}) {
  const response = useResource(() => lookup(identityId, { from }), [identityId, from]);

  return (
    <div
      data-testid={`lookup-${kind}-expansion-${identityId}`}
      className="rounded-md border bg-muted/40 p-2"
    >
      {response.loading && <p className="text-xs">loading</p>}
      {response.error && (
        <ErrorEnvelopeView error={response.error} testId={`lookup-${kind}-expansion-error`} />
      )}
      {response.data && <KnowledgeBody response={response.data} level={level} />}
    </div>
  );
}

function Degrees({ response, level }: { response: LookupResponse; level: number }) {
  const suffix = level === 0 ? "" : `-${response.identity.identity_id}`;
  return (
    <>
      {/* One statement, either way: the row when there is a distance to give,
          the sentence when there is not. */}
      {response.degrees === null ? (
        <p data-testid={`lookup-degrees-none${suffix}`} className="text-sm">
          <span data-testid={`lookup-degrees${suffix}`}>No connection found</span> yet. Sync and
          try again.
        </p>
      ) : (
        <KeyValueTable>
          <KeyValue label="how far away" testId={`lookup-degrees${suffix}`}>
            {response.degrees} {response.degrees === 1 ? "step" : "steps"}
          </KeyValue>
        </KeyValueTable>
      )}
    </>
  );
}

/**
 * One crawl answer: how a local identity reaches this one, who this one trusts,
 * and who this crawl saw attesting to it. The same body renders the section and
 * every expansion inside it, so no nested list can drop the best-effort label or
 * the staleness of the hop it came from.
 *
 * Testids repeat between an expansion and the list that opened it, on purpose:
 * every nested list sits inside its own `lookup-trust-expansion-<id>` or
 * `lookup-reverse-expansion-<id>`, which is what a reader of the DOM scopes by.
 */
export function KnowledgeBody({
  response,
  level,
}: {
  response: LookupResponse;
  level: number;
}) {
  const from = response.from.identity_id;
  const listed = [
    ...response.trust.map((entry) => entry.subject),
    ...response.reverse.entries.map((entry) => entry.identity),
  ];

  return (
    <div className="space-y-3">
      <Degrees response={response} level={level} />
      {level === 0 && response.paths.length > 0 && (
        <div data-testid="lookup-paths" className="space-y-2">
          {response.paths.map((path, index) => (
            <div
              key={path.hops.map((hop) => hop.attestation_event).join("-")}
              data-testid={`lookup-path-${index}`}
              className="rounded-md border px-2"
            >
              <ul className="divide-y">
                {path.hops.map((hop, hopIndex) => (
                  <Hop
                    key={hop.attestation_event}
                    hop={hop}
                    testId={`lookup-hop-${index}-${hopIndex}`}
                  />
                ))}
              </ul>
            </div>
          ))}
        </div>
      )}
      <IdentityListScope identities={listed}>
        <div className="space-y-1">
          <p className="text-xs text-muted-foreground">who they trust</p>
          {response.trust.length === 0 ? (
            <p data-testid="lookup-trust-empty" className="text-sm">
              Your wallet has not seen them trust anyone.
            </p>
          ) : (
            <ul data-testid="lookup-trust" className="divide-y">
              {response.trust.map((entry) => (
                <EntryRow
                  key={entry.attestation_event}
                  identity={entry.subject}
                  from={from}
                  level={level}
                  kind="trust"
                />
              ))}
            </ul>
          )}
        </div>
        <div className="space-y-1">
          <p data-testid="lookup-reverse-label" className="text-xs text-muted-foreground">
            {REVERSE_LABEL}
          </p>
          {response.reverse.entries.length === 0 ? (
            <p data-testid="lookup-reverse-empty" className="text-sm">
              Your wallet has not seen anyone trust them.
            </p>
          ) : (
            <ul data-testid="lookup-reverse" className="divide-y">
              {response.reverse.entries.map((entry) => (
                <EntryRow
                  key={entry.attestation_event}
                  identity={entry.identity}
                  from={from}
                  level={level}
                  kind="reverse"
                />
              ))}
            </ul>
          )}
        </div>
      </IdentityListScope>
    </div>
  );
}

/**
 * How you know them: the section a foreign identity's page carries. It answers
 * from one of your own identities, in named steps with the freshness of each,
 * and it says what your wallet has not seen rather than implying it does not
 * exist.
 */
export function KnowledgeSection({ response }: { response: LookupResponse }) {
  const sync = useGraphSync();
  // The paths already carry the equivocation of every node they reach, so the
  // heading only warns about one no hop is going to show.
  const onAPath = response.paths.some((path) =>
    path.hops.some(
      (hop) => hop.equivocation !== null && hop.to.identity_id === response.identity.identity_id,
    ),
  );

  return (
    <Card data-testid="lookup-result">
      <CardHeader>
        <CardTitle>How you know them</CardTitle>
        <CardDescription>
          <span className="inline-flex flex-wrap items-baseline gap-2">
            following trust out from your
            <IdentityInline identity={response.from} testId="lookup-from" />
          </span>
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
        <KnowledgeBody response={response} level={0} />
      </CardContent>
    </Card>
  );
}
