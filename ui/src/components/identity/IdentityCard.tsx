import { type ReactNode, useState } from "react";

import type {
  Contact,
  DeclaredKind,
  Identity,
  PrincipalEntry,
  ResolvedIdentity as ResolvedIdentityDocument,
  Verification,
} from "@/api/types";
import { DeclaredKindValue } from "@/components/DeclaredKind";
import { Identifier } from "@/components/Identifier";
import { NICKNAME_INFO, NOTE_INFO } from "@/components/InfoTip";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import {
  Collapsible,
  CollapsibleChevron,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { ICON_BUTTON } from "@/components/ui/icon-button";
import { formatDate } from "@/lib/time";
import { cn } from "@/lib/utils";

import { IdentityInline } from "./IdentityInline";
import { type Machine, machineSentence, machinesOf } from "./machines";
import { bareIdentity, IdentityListScope, resolvedFrom, VerificationMark } from "./names";
import { IdentityPillBadge, usePill } from "./pill";

/**
 * What the expanded card adds: the record itself, which only a screen holding
 * the identity document can fill in. A crawled identity has none of it, and the
 * card says so rather than printing zeroes.
 */
export interface IdentityRecord {
  /** The private nickname this device keeps, which never leaves it. */
  alias: string;
  createdAtMs: number;
  eventCount: number;
  verification: Verification;
  contact: Contact | null;
  /** How many identities it trusts and has not taken that back for. */
  trustedCount: number;
  principals: PrincipalEntry[];
  openInvitationCount: number;
  /** True when it holds no key of its own, so its controllers sign for it. */
  founded: boolean;
}

/** Everything either identity component needs about one identity. */
export interface IdentityFacts {
  resolved: ResolvedIdentityDocument;
  /** True when the identity document reports its verified result as aged. */
  stale: boolean;
  /** The email its profile publishes, null when it publishes none or none is known. */
  email: string | null;
  declaredKind: DeclaredKind | null;
  /** Where its id links, null when there is nowhere to go. */
  to: string | null;
  record: IdentityRecord | null;
  /**
   * Whether this home stores a copy of the record, and null when the screen
   * drawing the card does not know. A card that stores none says so in a pill,
   * because everything else on it came from a crawl; a card whose screen never
   * asked says nothing, rather than claiming an answer it does not have.
   */
  stored: boolean | null;
  /**
   * The newest position a listing reported for the record, null when none did.
   * It is how a stored copy this home does not control says how much of the
   * record it has, without the record itself being loaded.
   */
  headSeq: number | null;
  /**
   * The machines that answer for it, one row each. The identity document names
   * the ones its own record publishes; a screen holding the witness list adds
   * the ones this home knows from somewhere else.
   */
  machines: Machine[];
}

/** The facts an identity document carries, for every screen holding one. */
export function factsFromIdentity(identity: Identity, to: string | null = null): IdentityFacts {
  return {
    resolved: resolvedFrom(identity),
    stale: identity.verification.stale,
    email: identity.profile?.email ?? null,
    declaredKind: identity.declared_kind,
    to,
    stored: true,
    headSeq: identity.head_seq,
    machines: machinesOf(identity),
    record: {
      alias: identity.alias,
      createdAtMs: identity.created_at_ms,
      eventCount: identity.event_count,
      verification: identity.verification,
      contact: identity.contact,
      trustedCount: identity.trust.filter((record) => !record.revoked).length,
      principals: identity.principals,
      openInvitationCount: identity.open_invitation_count,
      // An identity-rooted ledger holds no key of its own, which is the one
      // fact about keys a reader can act on (proposal 002 section 2).
      founded: identity.active_key === undefined,
    },
  };
}

/**
 * The facts a crawled or witness-held identity carries: a name, an id, a verdict
 * and whatever summary the listing came with. The resolved document carries the
 * public email the crawl read, so a card built this way shows one without this
 * home holding the record; everything a record answers is missing until it does.
 */
export function factsFromResolved(
  resolved: ResolvedIdentityDocument,
  options: {
    declaredKind?: DeclaredKind | null;
    stale?: boolean;
    to?: string | null;
    /**
     * Whether this home holds a copy of the record behind the name. Left out on
     * a screen that never asked, and the card then says nothing about it.
     */
    stored?: boolean | null;
    /** The newest position the listing reported, when it reported one. */
    headSeq?: number | null;
    /** The machines a screen holding the witness list knows for it. */
    machines?: Machine[];
  } = {},
): IdentityFacts {
  return {
    resolved,
    stale: options.stale ?? false,
    email: resolved.email,
    declaredKind: options.declaredKind ?? null,
    to: options.to ?? null,
    record: null,
    stored: options.stored ?? null,
    headSeq: options.headSeq ?? null,
    machines: options.machines ?? [],
  };
}

/**
 * How much of the card is drawn. `collapsed` and `expanded` are the two halves
 * of one toggle; `page` is that same open block with no toggle at all, which is
 * what the identity page's top section is (proposal 005).
 */
export type IdentityCardState = "collapsed" | "expanded" | "page";

/** Builds the testids one card draws, so both conventions stay literal. */
export type CardTestIds = (part: string) => string;

const PAGE_PARTS: Record<string, string> = {
  "": "identity-detail",
  // The page's heading kept the name it had before the two components existed.
  name: "identity-detail-resolved",
};

/** The identity page's top section: `identity-detail` and its rows. */
export const pageTestIds: CardTestIds = (part) => PAGE_PARTS[part] ?? `identity-detail-${part}`;

/** One card in a list: `identity-card-<id>`, and `identity-card-<part>-<id>`. */
export function listTestIds(identityId: string): CardTestIds {
  return (part) =>
    part === "" ? `identity-card-${identityId}` : `identity-card-${part}-${identityId}`;
}

/** The record rows: the identity page's overview, and nothing else. */
function RecordRows({
  facts,
  testIds,
  resolvePrincipal,
}: {
  facts: IdentityFacts;
  testIds: CardTestIds;
  resolvePrincipal: (identityId: string) => ResolvedIdentityDocument;
}) {
  const record = facts.record;
  // One row per machine, each with its id and the sentence saying where the
  // claim came from. Never an id and a status on one line (decision 017).
  const machines = facts.machines.map((machine) => (
    <KeyValue
      key={machine.endpointId}
      label="machine"
      testId={testIds(`machine-${machine.endpointId}`)}
    >
      <span className="flex flex-col gap-1">
        <Identifier value={machine.endpointId} full copyLabel="Copy machine ID" />
        <span
          data-testid={testIds(`machine-${machine.endpointId}-note`)}
          className="text-xs text-muted-foreground"
        >
          {machineSentence(machine)}
        </span>
      </span>
    </KeyValue>
  ));
  // Every row label is lowercase, the public email included: one style for the
  // whole table, so no row reads as a heading over its neighbours.
  const email =
    facts.email === null ? null : (
      <KeyValue label="email" testId={testIds("email")}>
        <span className="break-all">{facts.email}</span>
      </KeyValue>
    );
  if (record === null) {
    // A copy this home stored says how much of the record it holds; one it did
    // not says exactly that, and a screen that never asked says neither.
    const entries = facts.headSeq === null ? null : facts.headSeq + 1;
    return (
      <KeyValueTable>
        {email}
        {machines}
        {(facts.stored === false || entries !== null) && (
          <KeyValue label="ledger" testId={testIds("ledger-summary")}>
            {entries === null ? (
              "your wallet holds no copy of it"
            ) : (
              <>
                <span data-testid={testIds("event-count")}>{entries}</span>{" "}
                {entries === 1 ? "entry" : "entries"}
              </>
            )}
          </KeyValue>
        )}
      </KeyValueTable>
    );
  }
  const contact = record.contact;
  const invitations = record.openInvitationCount;
  const nickname = contact?.nickname ?? record.alias;
  // A raw-rooted ledger is its own principal, and the card already holds the
  // name for that one: only the others need resolving.
  const principals = record.principals.map((principal) =>
    principal.identity === facts.resolved.identity_id
      ? facts.resolved
      : resolvePrincipal(principal.identity),
  );
  // An identity keyed by itself has one principal, itself, which the heading
  // above already names: the row is drawn when the answer differs from it. A
  // record with no principal set at all, which is what a fetched copy carries,
  // has no answer to give and says nothing.
  const principalsDiffer =
    principals.length > 1 ||
    (principals.length === 1 && principals[0].identity_id !== facts.resolved.identity_id);

  return (
    <KeyValueTable>
      {email}
      <KeyValue label="nickname" testId={testIds("alias")} info={NICKNAME_INFO}>
        {nickname === "" ? "none" : nickname}
      </KeyValue>
      <KeyValue label="note" testId={testIds("contact")} info={NOTE_INFO}>
        {contact?.note ?? "none"}
      </KeyValue>
      <KeyValue label="created" testId={testIds("created")}>
        {formatDate(record.createdAtMs)}
      </KeyValue>
      <KeyValue label="handle" testId={testIds("hostname")}>
        {record.verification.hostname === null ? (
          "none"
        ) : (
          <VerificationMark
            status={record.verification.status}
            hostname={record.verification.hostname}
            stale={record.verification.stale}
            recheckFailed={record.verification.unreachable !== null}
            testId={testIds("hostname-verification")}
          />
        )}
      </KeyValue>
      {machines}
      <KeyValue label="ledger" testId={testIds("ledger-summary")}>
        <span data-testid={testIds("event-count")}>{record.eventCount}</span>{" "}
        {record.eventCount === 1 ? "entry" : "entries"}
      </KeyValue>
      <KeyValue label="trusts" testId={testIds("trusted-count")}>
        {record.trustedCount} {record.trustedCount === 1 ? "identity" : "identities"}
      </KeyValue>
      {principalsDiffer && (
        <KeyValue label="who can act for it" testId={testIds("principals")}>
          <IdentityListScope identities={principals}>
            <span className="flex flex-col gap-1">
              {principals.map((principal) => (
                <IdentityInline
                  key={principal.identity_id}
                  identity={principal}
                  testId={testIds(`principal-${principal.identity_id}`)}
                  to={`/identities/${principal.identity_id}`}
                  // A raw-rooted ledger is its own only principal, and the card's
                  // heading already carries its pill: once is enough.
                  pill={principal.identity_id === facts.resolved.identity_id ? null : undefined}
                />
              ))}
              {/* The one key fact worth a sentence, said once, where it matters. */}
              {record.founded && (
                <span data-testid={testIds("founded")} className="text-xs text-muted-foreground">
                  Its controllers sign for it.
                </span>
              )}
            </span>
          </IdentityListScope>
        </KeyValue>
      )}
      <KeyValue label="invitations" testId={testIds("open-invitations")}>
        {invitations === 0
          ? "none"
          : `${invitations} waiting for an answer`}
      </KeyValue>
    </KeyValueTable>
  );
}

/**
 * One identity as a card, and one surface: the card draws the only border. The
 * name and what the identity says it is on one line, the pills and the expand
 * control at the end of that same line, and the id under it. The chevron opens
 * the record in place, and the same open block without the chevron is the
 * identity page's top section, whose name is the page's h1. One component, three
 * states, so a list entry and a page heading cannot drift apart (proposal 005).
 *
 * A card that routes somewhere is clickable across its whole surface: the name
 * is a real anchor whose box is stretched over the card, so the keyboard and a
 * screen reader reach the same page and nothing here listens for a click on a
 * div. Every control on the card sits above that anchor and keeps its own click.
 */
export function IdentityCard({
  facts,
  state = "collapsed",
  testIds,
  linkTestId,
  markers,
  resolvePrincipal = bareIdentity,
}: {
  facts: IdentityFacts;
  state?: IdentityCardState;
  testIds: CardTestIds;
  /** The testid on the id's link, for the lists a suite navigates by. */
  linkTestId?: string;
  /** Extra facts the listing carries, drawn on the kind line. */
  markers?: ReactNode;
  /** Names one principal, for a screen that resolved the ids it draws. */
  resolvePrincipal?: (identityId: string) => ResolvedIdentityDocument;
}) {
  const page = state === "page";
  const pill = usePill(facts.resolved.identity_id);
  const to = page ? null : facts.to;
  // The card holds the open state, because the short line above the name is the
  // closed card's version of a row the open one carries in full.
  const [open, setOpen] = useState(state === "expanded");
  const shown = page || open;
  // What the open block would add that the closed card does not already say: the
  // record, a public email, or how much of the record this home stored. A
  // crawled name with none of the three opens onto nothing, so it draws no
  // control at all: the pill in its corner is the whole answer.
  const expandable =
    facts.record !== null ||
    facts.email !== null ||
    facts.machines.length > 0 ||
    (facts.stored === true && facts.headSeq !== null);
  const extra = markers !== undefined && markers !== null && markers !== false;

  return (
    <Card
      data-testid={testIds("")}
      className={cn(
        // relative for the name's stretched link, and the ring so a card reached
        // by keyboard is as obvious as one under the pointer.
        "relative p-3 focus-within:ring-1 focus-within:ring-ring sm:p-4",
        to !== null &&
          "cursor-pointer transition-colors hover:border-foreground/30 hover:bg-accent/40",
      )}
    >
      <Collapsible open={shown} onOpenChange={setOpen}>
        <IdentityInline
          identity={facts.resolved}
          stale={facts.stale}
          testId={testIds("name")}
          linkTestId={linkTestId}
          to={to ?? undefined}
          stretch
          layout={page ? "page" : "stacked"}
          // The card draws the pill itself, at the end of the name line.
          pill={null}
          trailing={
            facts.declaredKind !== null && (
              <DeclaredKindValue kind={facts.declaredKind} testId={testIds("declared-kind")} />
            )
          }
          aside={
            <>
              {pill !== null && <IdentityPillBadge pill={pill} testId={`${testIds("name")}-pill`} />}
              {/* Everything on a card with no copy of the record came from a
                  crawl, and that is worth a pill beside the ones about trust. */}
              {facts.stored === false && (
                <Badge
                  variant="outline"
                  data-testid={testIds("unheld")}
                  className="bg-background"
                  title="Your wallet holds no copy of this record, only what a crawl read."
                >
                  not stored here
                </Badge>
              )}
              {!page && expandable && (
                <CollapsibleTrigger
                  data-testid={testIds("expand")}
                  aria-label={shown ? "Hide the record" : "Show the record"}
                  title={shown ? "Hide the record" : "Show the record"}
                  className={cn(ICON_BUTTON, "relative z-10")}
                >
                  <CollapsibleChevron className="size-4" />
                </CollapsibleTrigger>
              )}
            </>
          }
        />
        {/* What the listing that drew this card carries: how many entries a
            witness holds of it, how long ago a crawl saw it. */}
        {extra && (
          <p
            data-testid={testIds("kind-line")}
            className="mt-2 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground"
          >
            {markers}
          </p>
        )}
        <CollapsibleContent
          data-testid={testIds("details")}
          className="relative z-10 mt-4 border-t pt-4"
        >
          <RecordRows facts={facts} testIds={testIds} resolvePrincipal={resolvePrincipal} />
        </CollapsibleContent>
      </Collapsible>
    </Card>
  );
}

/** One entry of a card list. */
export interface IdentityCardEntry {
  facts: IdentityFacts;
  markers?: ReactNode;
}

/**
 * A list of identities, one card each, one column at every width. The scope is
 * the whole list, so two entries resolving to one name both drop the id
 * truncation and stay tellable apart.
 */
export function IdentityCardList({
  entries,
  testId,
  empty,
  emptyTestId = `${testId}-empty`,
  resolvePrincipal,
}: {
  entries: IdentityCardEntry[];
  testId: string;
  /** What the list says when it holds nothing. */
  empty: string;
  emptyTestId?: string;
  /** Names one principal, for a list whose screen resolved the ids it draws. */
  resolvePrincipal?: (identityId: string) => ResolvedIdentityDocument;
}) {
  if (entries.length === 0) {
    return (
      <p data-testid={emptyTestId} className="text-sm">
        {empty}
      </p>
    );
  }
  return (
    <IdentityListScope identities={entries.map((entry) => entry.facts.resolved)}>
      <ul data-testid={testId} className="grid gap-2">
        {entries.map((entry) => {
          const id = entry.facts.resolved.identity_id;
          return (
            <li key={id} className="min-w-0">
              <IdentityCard
                facts={entry.facts}
                testIds={listTestIds(id)}
                linkTestId={`identity-card-link-${id}`}
                markers={entry.markers}
                resolvePrincipal={resolvePrincipal}
              />
            </li>
          );
        })}
      </ul>
    </IdentityListScope>
  );
}
