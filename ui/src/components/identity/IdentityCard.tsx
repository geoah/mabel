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
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Card } from "@/components/ui/card";
import { formatDate } from "@/lib/time";

import { IdentityInline } from "./IdentityInline";
import { bareIdentity, IdentityListScope, resolvedFrom, VerificationMark } from "./names";

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
  headSeq: number;
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
  /** The newest position on its record, null when this screen does not know it. */
  headSeq: number | null;
  /** Where its id links, null when there is nowhere to go. */
  to: string | null;
  record: IdentityRecord | null;
}

/** The facts an identity document carries, for every screen holding one. */
export function factsFromIdentity(identity: Identity, to: string | null = null): IdentityFacts {
  return {
    resolved: resolvedFrom(identity),
    stale: identity.verification.stale,
    email: identity.profile?.email ?? null,
    declaredKind: identity.declared_kind,
    headSeq: identity.head_seq,
    to,
    record: {
      alias: identity.alias,
      createdAtMs: identity.created_at_ms,
      eventCount: identity.event_count,
      headSeq: identity.head_seq,
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
 * and whatever summary the listing came with. A profile email is not part of the
 * resolved document, so a card built this way shows none until this home holds
 * the record itself.
 */
export function factsFromResolved(
  resolved: ResolvedIdentityDocument,
  options: {
    declaredKind?: DeclaredKind | null;
    headSeq?: number | null;
    stale?: boolean;
    to?: string | null;
  } = {},
): IdentityFacts {
  return {
    resolved,
    stale: options.stale ?? false,
    email: null,
    declaredKind: options.declaredKind ?? null,
    headSeq: options.headSeq ?? null,
    to: options.to ?? null,
    record: null,
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
  if (record === null) {
    return (
      <KeyValueTable>
        <KeyValue label="ledger" testId={testIds("ledger-summary")}>
          your wallet holds no copy of it
        </KeyValue>
      </KeyValueTable>
    );
  }
  const contact = record.contact;
  const invitations = record.openInvitationCount;
  // A raw-rooted ledger is its own principal, and the card already holds the
  // name for that one: only the others need resolving.
  const principals = record.principals.map((principal) =>
    principal.identity === facts.resolved.identity_id
      ? facts.resolved
      : resolvePrincipal(principal.identity),
  );

  return (
    <KeyValueTable>
      <KeyValue label="your private nickname" testId={testIds("alias")}>
        {record.alias}
      </KeyValue>
      <KeyValue label="created" testId={testIds("created")}>
        {formatDate(record.createdAtMs)}
      </KeyValue>
      <KeyValue label="website" testId={testIds("hostname")}>
        {record.verification.hostname === null ? (
          "none"
        ) : (
          <VerificationMark
            status={record.verification.status}
            hostname={record.verification.hostname}
            stale={record.verification.stale}
            testId={testIds("hostname-verification")}
          />
        )}
      </KeyValue>
      <KeyValue label="your private note" testId={testIds("contact")}>
        {contact === null
          ? "none"
          : [contact.nickname, contact.note].filter((part) => part !== null).join(": ")}
      </KeyValue>
      <KeyValue label="ledger" testId={testIds("ledger-summary")}>
        <span data-testid={testIds("event-count")}>{record.eventCount}</span>{" "}
        {record.eventCount === 1 ? "entry" : "entries"}, the newest at position{" "}
        <span data-testid={testIds("head-seq")}>{record.headSeq}</span>
      </KeyValue>
      <KeyValue label="trusts" testId={testIds("trusted-count")}>
        {record.trustedCount} {record.trustedCount === 1 ? "identity" : "identities"}
      </KeyValue>
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
      <KeyValue label="invitations not yet answered" testId={testIds("open-invitations")}>
        {invitations === 0
          ? "none"
          : `${invitations} ${
              invitations === 1 ? "invitation" : "invitations"
            } to help control this identity, still waiting for an answer`}
      </KeyValue>
    </KeyValueTable>
  );
}

/**
 * One identity as a card: three or four lines by default, an expand control that
 * opens the record in place, and that same open block without the toggle as the
 * identity page's top section. One component, three states, so a list entry and
 * a page heading cannot drift apart (proposal 005).
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
  const [open, setOpen] = useState(state === "expanded");
  const shown = page || open;

  return (
    <Card data-testid={testIds("")} className="space-y-2 p-3 sm:p-4">
      <IdentityInline
        identity={facts.resolved}
        stale={facts.stale}
        testId={testIds("name")}
        linkTestId={linkTestId}
        to={facts.to ?? undefined}
      />
      {facts.email !== null && (
        <p data-testid={testIds("email")} className="text-sm break-all">
          {facts.email}
        </p>
      )}
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
        {facts.declaredKind !== null && (
          <DeclaredKindValue kind={facts.declaredKind} testId={testIds("declared-kind")} />
        )}
        {/* The open block says both of these in full, so the line holding the
            short version is drawn only while the card is closed. */}
        {!shown && facts.headSeq !== null && (
          <span data-testid={testIds("head-seq")}>at position {facts.headSeq}</span>
        )}
        {!shown && facts.headSeq === null && facts.record === null && (
          <span data-testid={testIds("unheld")}>no copy of its record here</span>
        )}
        {markers}
      </div>
      {!page && (
        <button
          type="button"
          data-testid={testIds("expand")}
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          className="flex min-h-9 items-center gap-1.5 text-xs underline focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          <span aria-hidden="true">{open ? "−" : "+"}</span>
          {open ? "Show less" : "Show more"}
        </button>
      )}
      {shown && (
        <div data-testid={testIds("details")} className="border-t pt-2">
          <RecordRows facts={facts} testIds={testIds} resolvePrincipal={resolvePrincipal} />
        </div>
      )}
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
}: {
  entries: IdentityCardEntry[];
  testId: string;
  /** What the list says when it holds nothing. */
  empty: string;
  emptyTestId?: string;
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
              />
            </li>
          );
        })}
      </ul>
    </IdentityListScope>
  );
}
