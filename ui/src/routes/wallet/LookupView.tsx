import { useState } from "react";

import { lookup } from "@/api/client";
import type {
  Equivocation,
  LookupHop,
  LookupResponse,
  ResolvedIdentity as ResolvedIdentityDocument,
} from "@/api/types";
import { DeveloperOnly } from "@/components/DeveloperMode";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { ResolvedIdentity, ResolvedIdentityScope } from "@/components/ResolvedIdentity";
import { Button } from "@/components/ui/button";
import { useResource } from "@/hooks/useResource";
import { describeAge, formatTimestamp } from "@/lib/time";

/**
 * How far a lookup expands in place. The screen itself is level 0, so a reader
 * can open a name and open one name inside it, and no further: a lookup that
 * kept expanding would walk the whole crawl (proposal 003 section 4).
 */
export const MAX_LEVEL = 2;

/**
 * The reverse list is never "who trusts them". It is who, in this one crawl,
 * was seen attesting to them, and it says so wherever it is drawn.
 */
export const REVERSE_LABEL =
  "best effort: who in this crawl attests to them, never who trusts them in the world";

/** Two signed events at one sequence, with the source that served each branch. */
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
        two signed events at seq{" "}
        <span data-testid={`${testId}-seq`}>{equivocation.at_seq}</span>, recorded by this crawl
        and never resolved to one branch
      </p>
      <ul className="space-y-1">
        {equivocation.branches.map((branch) => (
          <li key={branch.event} data-testid={`${testId}-branch-${branch.event}`}>
            <Identifier value={branch.event} />
            <DeveloperOnly>
              <span className="ml-2 text-muted-foreground">
                {branch.source.kind} <Identifier value={branch.source.endpoint} />
              </span>
            </DeveloperOnly>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** One edge of a path: who attested, to whom, and how fresh that reading is. */
function Hop({ hop, testId }: { hop: LookupHop; testId: string }) {
  return (
    <li data-testid={testId} className="flex flex-wrap items-center gap-x-2 gap-y-1 py-2">
      <ResolvedIdentity identity={hop.from} testId={`${testId}-from`} />
      <span className="text-xs text-muted-foreground">trusts</span>
      <ResolvedIdentity
        identity={hop.to}
        testId={`${testId}-to`}
        to={`/wallet/lookup/${hop.to.identity_id}`}
      />
      <span data-testid={`${testId}-fetched`} className="text-xs text-muted-foreground">
        read {describeAge(hop.fetched_at_ms)}
      </span>
      {hop.stale && (
        <span data-testid={`${testId}-stale`} className="text-xs italic">
          stale
        </span>
      )}
      <DeveloperOnly>
        <span className="w-full text-xs text-muted-foreground">
          attestation <Identifier value={hop.attestation_event} /> at{" "}
          {formatTimestamp(hop.fetched_at_ms)}
        </span>
      </DeveloperOnly>
      {hop.equivocation && (
        <EquivocationNotice
          equivocation={hop.equivocation}
          testId={`${testId}-equivocation`}
        />
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
  const [open, setOpen] = useState(false);
  const expandable = level < MAX_LEVEL;

  return (
    <li
      data-testid={`lookup-${kind}-row-${identity.identity_id}`}
      className="space-y-1 py-2"
    >
      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
        <ResolvedIdentity
          identity={identity}
          testId={`lookup-${kind}-name-${identity.identity_id}`}
          to={`/wallet/lookup/${identity.identity_id}`}
        />
        {expandable ? (
          <Button
            variant="outline"
            size="sm"
            className="ml-auto"
            data-testid={`lookup-${kind}-expand-${identity.identity_id}`}
            aria-expanded={open}
            onClick={() => setOpen(!open)}
          >
            {open ? "Close" : "Expand"}
          </Button>
        ) : (
          <span
            data-testid={`lookup-${kind}-expand-limit-${identity.identity_id}`}
            className="ml-auto text-xs text-muted-foreground"
          >
            two levels is the cap; open this name for its own lookup
          </span>
        )}
      </div>
      {open && expandable && (
        <Expansion identityId={identity.identity_id} from={from} level={level + 1} kind={kind} />
      )}
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
      {response.data && <LookupBody response={response.data} level={level} />}
    </div>
  );
}

function Degrees({ response, level }: { response: LookupResponse; level: number }) {
  const suffix = level === 0 ? "" : `-${response.identity.identity_id}`;
  return (
    <>
      <KeyValueTable>
        <KeyValue label="shortest path found in this crawl" testId={`lookup-degrees${suffix}`}>
          {response.degrees === null
            ? "none"
            : `${response.degrees} ${response.degrees === 1 ? "hop" : "hops"}`}
        </KeyValue>
      </KeyValueTable>
      {response.degrees === null && (
        <p data-testid={`lookup-degrees-none${suffix}`} className="text-sm">
          no path was found within this crawl's caps. That is an answer about this crawl, not
          about the world: it is not a statement that no relationship exists.
        </p>
      )}
    </>
  );
}

/**
 * One lookup answer: how the selected identity reaches this one, who this one
 * trusts, and who this crawl saw attesting to it. The same body renders the
 * screen and every expansion inside it, so no nested list can drop the
 * best-effort label or the staleness of the hop it came from.
 *
 * Testids repeat between an expansion and the list that opened it, on purpose:
 * every nested list sits inside its own `lookup-trust-expansion-<id>` or
 * `lookup-reverse-expansion-<id>`, which is what a reader of the DOM scopes by.
 */
export function LookupBody({
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
      {level === 0 && (
        <div className="space-y-2">
          {response.paths.length > 0 && (
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
        </div>
      )}
      <ResolvedIdentityScope identities={listed}>
        <div className="space-y-1">
          <p className="text-xs text-muted-foreground">who they trust</p>
          {response.trust.length === 0 ? (
            <p data-testid="lookup-trust-empty" className="text-sm">
              this crawl read no attestation of theirs
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
              this crawl saw nobody attesting to them
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
      </ResolvedIdentityScope>
    </div>
  );
}
