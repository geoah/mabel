import type { ApiError } from "@/api/client";
import { Alert } from "@/components/ui/alert";

/**
 * What each exit-code class of contracts/README.md, "The envelope", means to
 * the person who hit it. The code and the reason stay on screen beside this
 * sentence, because that is what a bug report needs.
 */
const CODE_MEANING: Record<number, string> = {
  2: "The node did not understand the request, or refused it because it did not come from this computer.",
  10: "Something in the request was the wrong shape.",
  20: "A signature, the record itself or a rule refused this.",
  30: "The other side could not be reached.",
  50: "Something changed this record first. Reload the page and try again.",
  60: "A key file on this computer can be read by other users. Fix its permissions.",
  70: "This version of mabel cannot do that.",
};

/**
 * The reasons whose own sentence beats their code class. node_unreachable is
 * minted in the client when fetch never got an answer: the request was not
 * refused and not misunderstood, so neither code sentence fits it.
 */
const REASON_MEANING: Record<string, string> = {
  node_unreachable: "The node did not answer. Is it running?",
};

function renderDetail(value: unknown): string {
  if (value === null) {
    return "null";
  }
  if (typeof value === "object") {
    return JSON.stringify(value);
  }
  return String(value);
}

/**
 * The error envelope, rendered field by field. Consumers branch on code and
 * details.reason, never on message, so both are shown separately.
 */
export function ErrorEnvelopeView({ error, testId = "error-envelope" }: {
  error: ApiError;
  testId?: string;
}) {
  const detailKeys = Object.keys(error.details).filter((key) => key !== "reason");
  return (
    <Alert variant="destructive" data-testid={testId}>
      <div className="flex items-center gap-2 text-xs">
        <span data-testid="error-code">code {error.code}</span>
        <span data-testid="error-status">status {error.status}</span>
        <span data-testid="error-reason">{error.reason}</span>
      </div>
      <p data-testid="error-message" className="mt-1 text-sm">
        {error.message}
      </p>
      <p data-testid="error-code-meaning" className="mt-1 text-xs">
        {REASON_MEANING[error.reason] ??
          CODE_MEANING[error.code] ??
          "The node reported a kind of failure this build does not recognise."}
      </p>
      {detailKeys.length > 0 && (
        <dl
          data-testid="error-details"
          className="mt-2 grid grid-cols-[10rem_1fr] gap-x-4 text-xs"
        >
          {detailKeys.map((key) => (
            <div key={key} className="col-span-2 grid grid-cols-subgrid">
              <dt className="text-muted-foreground">{key}</dt>
              <dd data-testid={`error-detail-${key}`} className="break-all font-mono">
                {renderDetail(error.details[key])}
              </dd>
            </div>
          ))}
        </dl>
      )}
    </Alert>
  );
}
