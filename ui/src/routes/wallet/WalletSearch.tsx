import { type FormEvent, useState } from "react";
import { useNavigate } from "react-router";

import { type ApiError, resolveInput } from "@/api/client";
import type { ResolveStatus } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { InlineField, InlineForm } from "@/components/InlineForm";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { asApiError } from "@/hooks/useResource";
import { LINK_MACHINES_PARAM } from "@/routes/identity/IdentityPage";

/** A 52-character lowercase base32 Mabel ID and nothing else. */
const IDENTITY_ID = /^[a-z2-7]{52}$/i;

/** The three answers that name no identity, which are the three this page draws. */
type NamelessStatus = Exclude<ResolveStatus, "resolved">;

/**
 * What a TXT lookup answered, worded so a reader knows it is about DNS and not
 * about the identity. The lookup navigates; it verifies nothing. A resolved
 * answer carries the id, so it opens that page instead of reading a sentence.
 */
const STATUS_SENTENCE: Record<NamelessStatus, string> = {
  no_record: "names no identity",
  mismatched_records: "answered, and nothing it said is a Mabel ID",
  unreachable: "gave no answer",
};

/**
 * The one box on the wallet front page, drawn bare under its heading: the page
 * is flat sections, not cards inside cards. A Mabel ID opens its page directly,
 * with no machines named. Anything else, a handle or a mabel:// link, is
 * resolved through the node, which either names an identity or says what the
 * TXT lookup answered: the browser parses no link of its own (proposal 006
 * section 7). A link's machines ride to the identity page, where the fetch
 * dials them and the page says first what that does.
 */
export function WalletSearch() {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [pending, setPending] = useState(false);
  const [status, setStatus] = useState<{ hostname: string; status: NamelessStatus } | null>(null);
  const [error, setError] = useState<ApiError | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const wanted = query.trim();
    setStatus(null);
    setError(null);
    if (wanted === "") {
      return;
    }
    if (IDENTITY_ID.test(wanted)) {
      void navigate(`/identities/${wanted.toLowerCase()}`);
      return;
    }
    setPending(true);
    try {
      const answer = await resolveInput(wanted);
      if (answer.identity_id !== null) {
        const machines = answer.endpoints.length === 0 ? "" : `?${LINK_MACHINES_PARAM}=${answer.endpoints.join(",")}`;
        void navigate(`/identities/${answer.identity_id}${machines}`);
        return;
      }
      // Only a resolved answer carries an id (contracts/README.md, "Resolve"),
      // so what is left here is one of the three that names none. A hostname is
      // the only kind that queries anything, so it is the only kind that lands
      // here with a status to read.
      if (answer.hostname !== null && answer.status !== null && answer.status !== "resolved") {
        setStatus({ hostname: answer.hostname, status: answer.status });
      }
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <div data-testid="wallet-search" className="space-y-2">
      <InlineForm onSubmit={submit} data-testid="wallet-search-form">
        <InlineField label="Mabel ID, handle or link" htmlFor="wallet-search-input">
          <Input
            id="wallet-search-input"
            data-testid="wallet-search-input"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="alice.example, or paste a Mabel ID or a link"
            className="font-mono text-xs"
          />
        </InlineField>
        <Button type="submit" data-testid="wallet-search-submit" disabled={pending}>
          {pending ? "resolving" : "Open"}
        </Button>
      </InlineForm>
      {status !== null && (
        <p data-testid="wallet-search-status" data-status={status.status} className="text-sm">
          <span className="font-mono text-xs">_mabel.{status.hostname}.</span>{" "}
          {STATUS_SENTENCE[status.status]}
        </p>
      )}
      {error && <ErrorEnvelopeView error={error} testId="wallet-search-error" />}
    </div>
  );
}
