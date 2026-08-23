import { type FormEvent, useState } from "react";
import { useNavigate } from "react-router";

import { type ApiError, resolveHostname } from "@/api/client";
import type { ResolveStatus } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { asApiError } from "@/hooks/useResource";

/** A 52-character lowercase base32 identity id and nothing else. */
const IDENTITY_ID = /^[a-z2-7]{52}$/i;

/**
 * What a TXT lookup answered, worded so a reader knows it is about DNS and not
 * about the identity. The lookup navigates; it verifies nothing.
 */
const STATUS_SENTENCE: Record<ResolveStatus, string> = {
  no_record: "holds no mabel record",
  mismatched_records: "holds records and none of them parses as an identity id",
  unreachable: "could not be answered by the resolver",
  resolved: "answered resolved without naming an identity id",
};

/**
 * The one box on the wallet front page. An identity id opens its page directly.
 * Anything else is treated as a hostname and resolved through the node, which
 * either names an identity or says what the TXT lookup answered.
 */
export function WalletSearch() {
  const navigate = useNavigate();
  const [query, setQuery] = useState("");
  const [pending, setPending] = useState(false);
  const [status, setStatus] = useState<{ hostname: string; status: ResolveStatus } | null>(null);
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
      const answer = await resolveHostname(wanted);
      if (answer.status === "resolved" && answer.identity_id !== null) {
        void navigate(`/identities/${answer.identity_id}`);
        return;
      }
      setStatus({ hostname: answer.hostname, status: answer.status });
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <Card data-testid="wallet-search">
      <CardHeader>
        <CardTitle>Open an identity</CardTitle>
        <CardDescription>An identity id, or a hostname to resolve through DNS</CardDescription>
      </CardHeader>
      <CardContent className="space-y-2">
        <form onSubmit={submit} className="flex flex-wrap items-end gap-2" data-testid="wallet-search-form">
          <div className="min-w-0 flex-1 space-y-1">
            <Label htmlFor="wallet-search-input">identity id or hostname</Label>
            <Input
              id="wallet-search-input"
              data-testid="wallet-search-input"
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              placeholder="52 base32 characters, or alice.example"
              className="font-mono text-xs"
            />
          </div>
          <Button type="submit" data-testid="wallet-search-submit" disabled={pending}>
            {pending ? "resolving" : "Open"}
          </Button>
        </form>
        {status !== null && (
          <p data-testid="wallet-search-status" data-status={status.status} className="text-sm">
            <span className="font-mono text-xs">_mabel.{status.hostname}.</span>{" "}
            {STATUS_SENTENCE[status.status]}
          </p>
        )}
        {error && <ErrorEnvelopeView error={error} testId="wallet-search-error" />}
      </CardContent>
    </Card>
  );
}
