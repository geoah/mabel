import { expect, type Locator, type Page } from "@playwright/test";

/**
 * The whole 52-character value of an identifier, read the way
 * docs/stories/README.md says to read one: the `data-value` attribute sits on
 * the `Identifier` span inside the element carrying the testid.
 */
export async function identifier(scope: Page | Locator, testId: string): Promise<string> {
  const value = scope.getByTestId(testId).locator("[data-value]").first();
  await expect(value).toBeAttached();
  const read = await value.getAttribute("data-value");
  if (read === null) {
    throw new Error(`${testId} has no data-value`);
  }
  return read;
}

/** Story 001 step 3: create one identity in a wallet UI and record its id. */
export async function createIdentity(
  page: Page,
  options: { alias: string; kind?: string; founder?: string },
): Promise<{ identityId: string; inceptionEvent: string }> {
  await page.getByTestId("identity-create-alias").fill(options.alias);
  if (options.kind) {
    await page.getByTestId("identity-create-declared-kind").selectOption(options.kind);
  }
  if (options.founder) {
    await page.getByTestId("identity-create-founder").fill(options.founder);
  }
  await page.getByTestId("identity-create-submit").click();
  await expect(page.getByTestId("identity-create-result-identity-id")).toBeVisible();
  return {
    identityId: await identifier(page, "identity-create-result-identity-id"),
    inceptionEvent: await identifier(page, "identity-create-result-inception-event"),
  };
}

/** Opens one identity's page from the wallet home. */
export async function openIdentity(page: Page, base: string, identityId: string): Promise<void> {
  await page.goto(`${base}/wallet`);
  await page.getByTestId(`identity-link-${identityId}`).click();
  await expect(page.getByTestId("identity-detail")).toBeVisible();
}

/** Story 001 step 6: name one witness on this identity's chain. */
export async function addWitness(
  page: Page,
  witnessEndpointId: string,
  expectedHeadSeq: number,
): Promise<void> {
  await page.getByTestId("witness-add-endpoint").fill(witnessEndpointId);
  await page.getByTestId("witness-add-submit").click();
  await expect(page.getByTestId("witness-add-head-seq")).toHaveText(`head_seq ${expectedHeadSeq}`);
  await expect(page.getByTestId(`witness-row-${witnessEndpointId}`)).toBeVisible();
}

/** Story 001 step 7: push to every configured witness and read the report. */
export async function push(
  page: Page,
  witnessEndpointId: string,
  expected: { stored: number; headSeq?: number },
): Promise<void> {
  await page.getByTestId("sync-push-submit").click();
  await expect(page.getByTestId("sync-push-report")).toBeVisible();
  await expect(page.getByTestId(`push-status-${witnessEndpointId}`)).toHaveText("accepted");
  await expect(page.getByTestId(`push-stored-${witnessEndpointId}`)).toHaveText(
    String(expected.stored),
  );
  if (expected.headSeq !== undefined) {
    await expect(page.getByTestId("sync-push-head-seq")).toHaveText(String(expected.headSeq));
  }
}

/** Story 001 step 8: attest trust in a subject and record the event id. */
export async function addTrust(page: Page, subject: string): Promise<string> {
  await page.getByTestId("trust-add-subject").fill(subject);
  await page.getByTestId("trust-add-submit").click();
  await expect(page.getByTestId("trust-appended-event")).toBeVisible();
  return identifier(page, "trust-appended-event");
}

/** Story 003 step 7: run one trust verification on the wallet's verify page. */
export async function verifyTrustInUi(
  page: Page,
  base: string,
  fields: { issuer: string; subject: string; from: string },
): Promise<void> {
  await page.goto(`${base}/wallet`);
  await page.getByTestId("nav-verify").click();
  await page.getByTestId("verify-trust-issuer").fill(fields.issuer);
  await page.getByTestId("verify-trust-subject").fill(fields.subject);
  await page.getByTestId("verify-trust-from").fill(fields.from);
  await page.getByTestId("verify-trust-submit").click();
  await expect(page.getByTestId("verify-report")).toBeVisible();
}
