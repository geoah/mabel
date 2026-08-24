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

/**
 * Opens the create form, which the wallet home folds away behind
 * `identity-create-summary` (proposal 004). Clicking a summary toggles it, so
 * the click is skipped when the form is already on the screen.
 */
export async function openCreateForm(page: Page): Promise<void> {
  const form = page.getByTestId("identity-create-form");
  if (!(await form.isVisible())) {
    await page.getByTestId("identity-create-summary").click();
  }
  await expect(form).toBeVisible();
}

/**
 * Opens one action on the identity page. Every action starts closed (decision
 * 017), and clicking a summary toggles it, so the click is skipped when the
 * action is already open: a helper called twice on one page must not close what
 * the first call opened.
 */
export async function openAction(page: Page, testId: string): Promise<void> {
  const action = page.getByTestId(testId);
  await expect(action).toBeVisible();
  if (!(await action.evaluate((element) => (element as HTMLDetailsElement).open))) {
    await page.getByTestId(`${testId}-summary`).click();
  }
  await expect(action).toHaveJSProperty("open", true);
}

/** Story 001 step 3: create one identity in a wallet UI and record its id. */
export async function createIdentity(
  page: Page,
  options: { alias: string; kind?: string; founder?: string },
): Promise<{ identityId: string; inceptionEvent: string }> {
  await openCreateForm(page);
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

/**
 * Opens one identity's page from the wallet home, by clicking its card. The
 * whole card is one link, and the page it opens is `/identities/<id>`: one
 * identity is one page, local or foreign (proposal 004).
 */
export async function openIdentity(page: Page, base: string, identityId: string): Promise<void> {
  await page.goto(`${base}/wallet`);
  await page.getByTestId(`identity-card-link-${identityId}`).click();
  await expect(page).toHaveURL(`${base}/identities/${identityId}`);
  await expect(page.getByTestId("identity-detail")).toBeVisible();
}

/**
 * Opens any identity's page through the one search box on the wallet home. An
 * identity id navigates without asking the node anything; a hostname is
 * resolved through `GET /api/resolve/<hostname>` first.
 */
export async function searchIdentity(
  page: Page,
  base: string,
  query: string,
  expectedId: string,
): Promise<void> {
  await page.goto(`${base}/wallet`);
  await expect(page.getByTestId("wallet-search")).toBeVisible();
  await page.getByTestId("wallet-search-input").fill(query);
  await page.getByTestId("wallet-search-submit").click();
  await expect(page).toHaveURL(`${base}/identities/${expectedId}`);
}

/** Story 001 step 6: name one witness on this identity's chain. */
export async function addWitness(
  page: Page,
  witnessEndpointId: string,
  expectedHeadSeq: number,
): Promise<void> {
  await openAction(page, "action-witnesses");
  await page.getByTestId("witness-add-endpoint").fill(witnessEndpointId);
  await page.getByTestId("witness-add-submit").click();
  await expect(page.getByTestId("witness-add-head-seq")).toHaveText(
    `Saved at position ${expectedHeadSeq}.`,
  );
  await expect(page.getByTestId(`witness-row-${witnessEndpointId}`)).toBeVisible();
}

/**
 * Clicks one submit button and waits for the request it fires to answer.
 *
 * `click` resolves once the event is dispatched, before the browser has
 * rendered anything, so a helper called twice on one page can assert against
 * the result the previous call left on the screen. Waiting for the response
 * puts the assertions after the render that replaces it.
 */
async function submitAndAwait(page: Page, testId: string, route: string): Promise<void> {
  const answered = page.waitForResponse(
    (response) => response.url().endsWith(route) && response.request().method() === "POST",
  );
  await page.getByTestId(testId).click();
  await answered;
}

/** Story 001 step 7: push to every configured witness and read the report. */
export async function push(
  page: Page,
  witnessEndpointId: string,
  expected: { stored: number; headSeq?: number },
): Promise<void> {
  await openAction(page, "action-push");
  await submitAndAwait(page, "sync-push-submit", "/api/sync/push");
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
  await openAction(page, "action-trust");
  await page.getByTestId("trust-add-subject").fill(subject);
  await submitAndAwait(page, "trust-add-submit", "/api/trust");
  await expect(page.getByTestId("trust-appended-event")).toBeVisible();
  return identifier(page, "trust-appended-event");
}

/**
 * The ids of the identity cards on the screen now, in the order they are drawn.
 * `identity-card-link-<id>` is the one testid a card carries exactly once.
 */
export async function cardIds(page: Page): Promise<string[]> {
  await expect(page.getByTestId("identity-cards")).toBeVisible();
  const testIds = await page
    .locator('[data-testid^="identity-card-link-"]')
    .evaluateAll((elements) =>
      elements.map((element) => element.getAttribute("data-testid") ?? ""),
    );
  return testIds.map((testId) => testId.replace("identity-card-link-", ""));
}
