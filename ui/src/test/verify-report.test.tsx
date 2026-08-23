import { screen } from "@testing-library/react";
import { http } from "msw";
import { describe, expect, it } from "vitest";

import type { VerifyTrustReport } from "@/api/types";
import {
  ALICE,
  BOB,
  verifyLedgerPartial,
  verifyLedgerValid,
  verifyTrustRevoked,
  verifyTrustTrusted,
  verifyTrustUnresolved,
} from "@/mocks/fixtures";
import { server } from "@/mocks/server";

import { renderApp } from "./render";

function serveReport(document: unknown, status = 200) {
  server.use(http.post("/api/verify", () => Response.json(document, { status })));
}

async function submitTrust() {
  const { user } = renderApp("/wallet/verify");
  await user.type(screen.getByTestId("verify-trust-issuer"), ALICE);
  await user.type(screen.getByTestId("verify-trust-subject"), BOB);
  await user.click(screen.getByTestId("verify-trust-submit"));
  return screen.findByTestId("verify-report");
}

async function submitLedger() {
  const { user } = renderApp("/wallet/verify");
  await user.type(screen.getByTestId("verify-ledger-id"), ALICE);
  await user.click(screen.getByTestId("verify-ledger-submit"));
}

describe("verify report", () => {
  it("renders the trusted report with the statement verbatim", async () => {
    serveReport(verifyTrustTrusted);
    await submitTrust();

    // Flag R: the node renders the sentence, the UI prints it unchanged.
    expect(screen.getByTestId("verify-report-statement")).toHaveTextContent(
      verifyTrustTrusted.statement,
    );
    expect(screen.getByTestId("verify-report-trusted")).toHaveTextContent("true");
    expect(screen.getByTestId("verify-report-source")).toHaveTextContent(
      verifyTrustTrusted.source,
    );
    expect(screen.getByTestId("verify-report-head-seq")).toHaveTextContent("2");
    expect(screen.getByTestId("verify-report-head-event")).toHaveTextContent(
      verifyTrustTrusted.head_event,
    );
    expect(screen.getByTestId("verify-report-fetched-at-ms")).toHaveTextContent("1700000500000");
    expect(screen.getByTestId("verify-report-sources-queried")).toHaveTextContent(
      verifyTrustTrusted.sources_queried[1],
    );
    expect(screen.getByTestId("verify-report-attestation-seq")).toHaveTextContent("2");
    expect(screen.getByTestId("verify-report-revoked-count")).toHaveTextContent("0");
    // Flag L, verbatim.
    expect(screen.getByTestId("verify-report-subject-control")).toHaveTextContent(
      verifyTrustTrusted.subject_control,
    );
    expect(screen.getByTestId("verify-report-verified-means")).toHaveTextContent(
      verifyTrustTrusted.verified_means,
    );
    expect(screen.queryByTestId("verify-report-signing-principal")).not.toBeInTheDocument();
  });

  it("renders the revoked report with every revoked attestation", async () => {
    serveReport(verifyTrustRevoked);
    await submitTrust();

    expect(screen.getByTestId("verify-report-trusted")).toHaveTextContent("false");
    expect(screen.getByTestId("verify-report-statement")).toHaveTextContent(
      verifyTrustRevoked.statement,
    );
    expect(screen.getByTestId("verify-report-head-seq")).toHaveTextContent("3");
    expect(screen.getByTestId("verify-report-attestation-event")).toHaveTextContent("null");
    expect(screen.getByTestId("verify-report-revoked-count")).toHaveTextContent("1");
    const revoked = verifyTrustRevoked.revoked_attestations[0];
    expect(
      screen.getByTestId(`verify-report-revoked-${revoked.attestation_event}`),
    ).toHaveTextContent(String(revoked.revocation_seq));
  });

  it("renders the unresolved subject with its note", async () => {
    serveReport(verifyTrustUnresolved);
    await submitTrust();

    expect(screen.getByTestId("verify-report-subject-resolution")).toHaveTextContent(
      "unresolved",
    );
    expect(screen.getByTestId("verify-report-subject-note")).toHaveTextContent(
      verifyTrustUnresolved.subject_note ?? "",
    );
    expect(screen.getByTestId("verify-report-statement")).toHaveTextContent(
      verifyTrustUnresolved.statement,
    );
  });

  it("names the signing principal when the report carries one", async () => {
    const withPrincipal: VerifyTrustReport = {
      ...verifyTrustTrusted,
      signing_principal: BOB,
    };
    serveReport(withPrincipal);
    await submitTrust();

    expect(screen.getByTestId("verify-report-signing-principal")).toHaveTextContent(BOB);
  });

  it("renders the ledger report with the declared kind called advisory", async () => {
    serveReport(verifyLedgerValid);
    await submitLedger();

    await screen.findByTestId("verify-report");
    expect(screen.getByTestId("verify-report-kind")).toHaveTextContent("ledger");
    expect(screen.getByTestId("verify-report-declared-kind")).toHaveTextContent("person");
    expect(screen.getByTestId("verify-report-declared-kind-note")).toHaveTextContent(
      "declared kind is advisory",
    );
    expect(screen.getByTestId("verify-report-statement")).toHaveTextContent(
      verifyLedgerValid.statement,
    );
    expect(screen.getByTestId("verify-report-valid-to-seq")).toHaveTextContent("3");
    expect(screen.queryByTestId("verify-report-subject-control")).not.toBeInTheDocument();
  });

  it("renders partial validity as a code 20 failure carrying the report fields", async () => {
    serveReport(verifyLedgerPartial, 409);
    await submitLedger();

    const envelope = await screen.findByTestId("verify-error");
    expect(envelope).toHaveTextContent("code 20");
    expect(screen.getByTestId("error-reason")).toHaveTextContent("invalid_signature");
    expect(screen.getByTestId("error-detail-valid_to_seq")).toHaveTextContent("2");
    expect(screen.getByTestId("error-detail-failed_at_seq")).toHaveTextContent("3");
    expect(screen.getByTestId("error-detail-statement")).toHaveTextContent(
      String(verifyLedgerPartial.details.statement),
    );
    expect(screen.queryByTestId("verify-report")).not.toBeInTheDocument();
  });
});
