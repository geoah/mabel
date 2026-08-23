import type { ReactNode } from "react";

import { useDeveloperMode } from "@/lib/preferences";

/**
 * Developer mode, off by default (decision 014). It hides nothing from the
 * product: head event ids, witness endpoint ids, principal keys, sync
 * freshness, crawl provenance and the raw response document are all one toggle
 * away, and the primary view stays an address book while it is off.
 */
export function DeveloperOnly({ children }: { children: ReactNode }) {
  const [developer] = useDeveloperMode();
  if (!developer) {
    return null;
  }
  return <>{children}</>;
}

/** The raw response document, the last thing the toggle reveals. */
export function RawDocument({ value, testId }: { value: unknown; testId: string }) {
  return (
    <DeveloperOnly>
      <pre
        data-testid={testId}
        className="max-h-80 overflow-auto rounded-md border bg-muted p-2 font-mono text-xs"
      >
        {JSON.stringify(value, null, 2)}
      </pre>
    </DeveloperOnly>
  );
}
