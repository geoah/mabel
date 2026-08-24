import { screen, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { ApiError } from "@/api/client";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { usePagedList } from "@/hooks/usePagedList";
import { useResource } from "@/hooks/useResource";

import { renderComponent } from "./render";

function refusal(message: string): ApiError {
  return new ApiError(
    { ok: false, code: 20, message, details: { reason: "ledger_invalid" } },
    409,
  );
}

/** One resource, its document, its error and its reload button, and nothing else. */
function Probe({ load }: { load: () => Promise<string> }) {
  const resource = useResource(load, []);
  return (
    <div>
      {resource.data !== null && <p data-testid="probe-data">{resource.data}</p>}
      {resource.error && <ErrorEnvelopeView error={resource.error} testId="probe-error" />}
      <button type="button" data-testid="probe-reload" onClick={resource.reload}>
        reload
      </button>
    </div>
  );
}

describe("useResource", () => {
  it("keeps the last document on a failed reload, and reports the failure beside it", async () => {
    let attempt = 0;
    const load = () => {
      attempt += 1;
      return attempt === 1
        ? Promise.resolve("the answer the node gave")
        : Promise.reject(refusal("Ledger error: the reload was refused"));
    };
    const { user } = renderComponent(<Probe load={load} />);
    await screen.findByTestId("probe-data");

    await user.click(screen.getByTestId("probe-reload"));

    const envelope = await screen.findByTestId("probe-error");
    // Both are true and both are on screen: the document is real, and so is the
    // failure to refresh it.
    expect(screen.getByTestId("probe-data")).toHaveTextContent("the answer the node gave");
    expect(envelope).toHaveTextContent("Ledger error: the reload was refused");
  });

  it("holds no document at all when the first read fails", async () => {
    renderComponent(<Probe load={() => Promise.reject(refusal("Ledger error: refused"))} />);

    await screen.findByTestId("probe-error");
    expect(screen.queryByTestId("probe-data")).not.toBeInTheDocument();
  });
});

/** One paged list, its items, whether the cap cut it short, and its error. */
function PagedProbe({
  load,
  cap = 3,
  pageSize = 2,
}: {
  load: (offset: number, limit: number) => Promise<{ items: string[]; more: boolean }>;
  cap?: number;
  pageSize?: number;
}) {
  const page = usePagedList(load, [], { cap, pageSize });
  return (
    <div>
      <p data-testid="probe-items">{page.items.join(",")}</p>
      {page.capped && <p data-testid="probe-capped">capped</p>}
      {page.error && <ErrorEnvelopeView error={page.error} testId="probe-error" />}
    </div>
  );
}

describe("usePagedList", () => {
  it("reads every page the route serves", async () => {
    const pages = [
      { items: ["a", "b"], more: true },
      { items: ["c"], more: false },
    ];
    const asked: number[] = [];
    const load = (offset: number) => {
      asked.push(offset);
      return Promise.resolve(pages[asked.length - 1]);
    };
    renderComponent(<PagedProbe load={load} cap={64} />);

    await waitFor(() => expect(screen.getByTestId("probe-items")).toHaveTextContent("a,b,c"));
    expect(asked).toEqual([0, 2]);
    expect(screen.queryByTestId("probe-capped")).not.toBeInTheDocument();
  });

  it("stops at the cap and says the list is not the whole answer", async () => {
    const load = (offset: number, limit: number) =>
      Promise.resolve({
        items: Array.from({ length: limit }, (_, index) => String(offset + index)),
        more: true,
      });
    renderComponent(<PagedProbe load={load} cap={3} pageSize={2} />);

    await waitFor(() => expect(screen.getByTestId("probe-capped")).toBeInTheDocument());
    expect(screen.getByTestId("probe-items")).toHaveTextContent("0,1,2");
  });

  it("keeps the pages it read when a later page fails", async () => {
    let attempt = 0;
    const load = () => {
      attempt += 1;
      return attempt === 1
        ? Promise.resolve({ items: ["a", "b"], more: true })
        : Promise.reject(refusal("Ledger error: the second page was refused"));
    };
    renderComponent(<PagedProbe load={load} cap={64} />);

    await screen.findByTestId("probe-error");
    expect(screen.getByTestId("probe-items")).toHaveTextContent("a,b");
  });
});
