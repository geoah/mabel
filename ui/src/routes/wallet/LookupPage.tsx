import { type FormEvent, useState } from "react";
import { useNavigate, useParams } from "react-router";

import { getContact, listIdentities, lookup } from "@/api/client";
import { Action } from "@/components/Action";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { ResolvedIdentity } from "@/components/ResolvedIdentity";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useResource } from "@/hooks/useResource";
import { useSelectedIdentity } from "@/lib/preferences";

import { ContactPanel } from "./ContactPanel";
import { GraphStatusPanel } from "./GraphStatusPanel";
import { GraphStalenessBanner, useGraphSync } from "./GraphSyncControl";
import { IdentitySelector } from "./IdentitySelector";
import { EquivocationNotice, LookupBody } from "./LookupView";

/**
 * The private note this home keeps about the identity being looked up. The
 * contact store covers foreign ids, which is the whole point of it: a nickname
 * for someone whose ledger this wallet does not hold (proposal 003 section 1).
 */
function LookupContact({ identityId }: { identityId: string }) {
  const [version, setVersion] = useState(0);
  const contact = useResource(() => getContact(identityId), [identityId, version]);

  return (
    <Action
      testId="lookup-contact"
      title="Edit the contact note"
      description="A private nickname and note kept in this node home, never signed and never synced."
    >
      {contact.error && <ErrorEnvelopeView error={contact.error} testId="lookup-contact-error" />}
      {contact.data && (
        <ContactPanel
          key={version}
          identityId={identityId}
          contact={contact.data.contact}
          onSaved={() => setVersion((value) => value + 1)}
        />
      )}
    </Action>
  );
}

/** One lookup, answered relative to the identity the selector holds. */
function LookupResult({ identityId, from }: { identityId: string; from: string }) {
  const response = useResource(() => lookup(identityId, { from }), [identityId, from]);
  const sync = useGraphSync();
  const answer = response.data;
  // The paths already carry the equivocation of every node they reach, so the
  // heading only warns about one no hop is going to show.
  const onAPath =
    answer?.paths.some((path) =>
      path.hops.some(
        (hop) => hop.equivocation !== null && hop.to.identity_id === answer.identity.identity_id,
      ),
    ) ?? false;

  return (
    <Card data-testid="lookup-result">
      <CardHeader>
        <CardTitle className="text-base">
          {answer ? (
            <ResolvedIdentity identity={answer.identity} testId="lookup-identity" />
          ) : (
            "Lookup"
          )}
        </CardTitle>
        <CardDescription>
          {answer ? (
            <span className="inline-flex flex-wrap items-baseline gap-2">
              from
              <ResolvedIdentity identity={answer.from} testId="lookup-from" />
            </span>
          ) : (
            "how the selected identity knows this one"
          )}
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-3">
        {response.loading && <p data-testid="lookup-loading">loading</p>}
        {response.error && <ErrorEnvelopeView error={response.error} testId="lookup-error" />}
        {answer && (
          <>
            <GraphStalenessBanner
              stale={answer.graph_stale}
              lastSyncMs={answer.last_sync_ms}
              sync={sync}
              testId="lookup-graph-stale"
            />
            {answer.graph_truncated && (
              <details data-testid="lookup-graph-truncated" className="rounded-md border">
                <summary className="flex min-h-11 cursor-pointer list-none items-center px-3 py-2 text-sm marker:content-none hover:bg-accent">
                  <span>
                    the crawl behind this answer stopped early, truncated by{" "}
                    <span className="font-mono text-xs">{answer.truncated_by}</span>
                  </span>
                </summary>
                <p className="border-t px-3 py-2 text-sm">
                  A truncated crawl reached fewer identities than exist. A missing path here
                  means this crawl found none, not that none exists.
                </p>
              </details>
            )}
            {answer.equivocation && !onAPath && (
              <EquivocationNotice
                equivocation={answer.equivocation}
                testId="lookup-equivocation"
              />
            )}
            <LookupBody response={answer} level={0} />
            <LookupContact identityId={answer.identity.identity_id} />
          </>
        )}
      </CardContent>
    </Card>
  );
}

/**
 * The lookup screen of proposal 003 section 4: a foreign identity answered from
 * one local root, with the path in named hops, their trust list and the
 * best-effort reverse list. The root is the identity the selector holds, so
 * changing the selection re-asks the question from someone else.
 */
export function LookupPage() {
  const { identityId } = useParams();
  const navigate = useNavigate();
  const identities = useResource(listIdentities, []);
  const [stored] = useSelectedIdentity();
  const [query, setQuery] = useState(identityId ?? "");
  const local = identities.data?.identities ?? [];
  const from =
    local.find((identity) => identity.identity_id === stored)?.identity_id ??
    local[0]?.identity_id ??
    null;

  function submit(event: FormEvent) {
    event.preventDefault();
    const wanted = query.trim();
    if (wanted !== "") {
      void navigate(`/wallet/lookup/${wanted}`);
    }
  }

  return (
    <div className="grid gap-4 lg:grid-cols-2">
      <Card className="lg:col-span-2" data-testid="lookup-form-card">
        <CardHeader>
          <CardTitle>Look an identity up</CardTitle>
          <CardDescription>
            How do I know this identity, answered from one of this wallet's identities
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={submit} className="flex flex-wrap items-end gap-2" data-testid="lookup-form">
            <div className="min-w-0 flex-1 space-y-1">
              <Label htmlFor="lookup-identity-id">identity id</Label>
              <Input
                id="lookup-identity-id"
                data-testid="lookup-identity-id"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="52 base32 characters"
                className="font-mono text-xs"
              />
            </div>
            <Button type="submit" data-testid="lookup-submit">
              Look up
            </Button>
          </form>
        </CardContent>
      </Card>
      {identities.data && (
        <div className="lg:col-span-2">
          <IdentitySelector identities={identities.data.identities} />
        </div>
      )}
      {identityId !== undefined && from !== null && (
        <div className="lg:col-span-2">
          <LookupResult identityId={identityId} from={from} />
        </div>
      )}
      {identityId !== undefined && from === null && identities.data && (
        <p data-testid="lookup-no-root" className="lg:col-span-2">
          this node home holds no identity to look up from
        </p>
      )}
      <div className="lg:col-span-2">
        <GraphStatusPanel />
      </div>
    </div>
  );
}
