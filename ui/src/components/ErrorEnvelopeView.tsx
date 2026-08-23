import type { ApiError } from "@/api/client";
import { Alert } from "@/components/ui/alert";

/** The exit-code classes of contracts/README.md, "The envelope". */
const CODE_MEANING: Record<number, string> = {
  2: "usage, unknown route or parameter, rejected by the loopback rules",
  10: "invalid schema or malformed input",
  20: "cryptographic, chain or policy failure",
  30: "peer or network unavailable",
  50: "stale state, a conflicting event or a replay",
  60: "insecure key file permissions",
  70: "unsupported feature or version",
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
        {CODE_MEANING[error.code] ?? "unknown code"}
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
