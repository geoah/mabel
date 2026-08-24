#!/usr/bin/env node
// Captures every route at three widths and reports any page that scrolls
// sideways. The images land in ui/screenshots/, which is ignored by git: they
// exist to be looked at, not to be committed.
//
//   npm run screenshots
//
// It builds and serves the harness itself, from vite.harness.config.ts: the
// wallet against the frozen fixtures through the mock service worker, into
// ui/dist-harness/, which no binary and no release ever reads. `npm run build`
// writes ui/dist/ from a different config and cannot reach a fixture.
//
// BASE_URL points the run at an already-running server instead, and then
// nothing is built: use it to capture a real node's screens.

import { mkdir, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright";
import { build, preview } from "vite";

const HARNESS_CONFIG = fileURLToPath(new URL("../vite.harness.config.ts", import.meta.url));
const HARNESS_PORT = 4199;
const OUT_DIR = fileURLToPath(new URL("../screenshots", import.meta.url));

/** The ids the fixtures carry; the same ones the component tests use. */
const ALICE = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";
const BOB = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";
/** The organization the fixtures found: identity-rooted, holding no key. */
const ACME = "2okqwhextnpkpmydrgrkk563vbehcklffwfzidxlh5dslawjmn6a";
/** The foreign identity the lookup fixture answers for, and no witness holds. */
const CAROL = "jqtnsb2me7mj5xsze4gavqklohqhdmkshfiz65khjmxtxjruqh2q";
/** A record one witness holds and this home stores no copy of, so a fetch has work. */
const UNSTORED_LEDGER = "cd".repeat(26);
/** The two witness endpoints the fixtures carry: one answers, one does not. */
const WITNESS = "zbj22dym2k3btlvjftxmj7kwujgwjgovqthhsjl6ixh5qe43mctq";
const UNREACHABLE_WITNESS = "54rw3lmckcpqf4ofkvyx3i74agumvale2qmzdu76ubpita6sw5va";

const VIEWPORTS = [
  { name: "360x780", width: 360, height: 780 },
  { name: "768x1024", width: 768, height: 1024 },
  { name: "1280x800", width: 1280, height: 800 },
];

const SCREENS = [
  { name: "wallet-home", path: "/wallet", ready: "identity-cards" },
  {
    name: "wallet-home-create",
    path: "/wallet",
    ready: "identity-create-summary",
    async act(page) {
      await page.getByTestId("identity-create-summary").click();
      await page.getByTestId("identity-create-submit").waitFor();
    },
  },
  {
    // The step decision 017 asks for: the two keys, offered right after a create.
    name: "wallet-home-create-keys",
    path: "/wallet",
    ready: "identity-create-summary",
    async act(page) {
      await page.getByTestId("identity-create-summary").click();
      await page.getByTestId("identity-create-alias").fill("dana");
      await page.getByTestId("identity-create-submit").click();
      await page.getByTestId("identity-keys-download").waitFor();
    },
  },
  {
    // The second list: every identity this wallet knows of and does not
    // control, narrowed to the ones it has a reason to trust.
    name: "wallet-home-known-trusted",
    path: "/wallet",
    ready: "known-identity-cards",
    async act(page) {
      await page.getByTestId("known-trusted-only").click();
      await page.getByTestId("known-identity-cards").waitFor();
    },
  },
  {
    // A known identity with a copy of the record on disk, opened in place.
    name: "wallet-home-known-expanded",
    path: "/wallet",
    ready: "known-identity-cards",
    async act(page) {
      await page.getByTestId(`identity-card-expand-${BOB}`).click();
      await page.getByTestId(`identity-card-details-${BOB}`).waitFor();
    },
  },
  {
    // The card's middle state: opened in place, which is the identity page's
    // top section drawn inside a list entry (proposal 005).
    name: "wallet-home-card-expanded",
    path: "/wallet",
    ready: `identity-card-expand-${ALICE}`,
    async act(page) {
      await page.getByTestId(`identity-card-expand-${ALICE}`).click();
      await page.getByTestId(`identity-card-details-${ALICE}`).waitFor();
    },
  },
  {
    // The info icon beside a short label, opened: what a phone gets instead of
    // a hover, and the only long sentence either local field carries.
    name: "wallet-home-card-info",
    path: "/wallet",
    ready: `identity-card-expand-${ALICE}`,
    async act(page) {
      await page.getByTestId(`identity-card-expand-${ALICE}`).click();
      await page.getByTestId(`identity-card-alias-${ALICE}-info`).click();
      await page.getByTestId(`identity-card-alias-${ALICE}-info-text`).waitFor();
    },
  },
  {
    name: "wallet-home-resolve",
    path: "/wallet",
    ready: "wallet-search-input",
    async act(page) {
      await page.getByTestId("wallet-search-input").fill("nobody.example");
      await page.getByTestId("wallet-search-submit").click();
      await page.getByTestId("wallet-search-status").waitFor();
    },
  },
  { name: "identity-own", path: `/identities/${ALICE}`, ready: "ledger-events" },
  {
    // An identity founded by another one: it holds no key, and its record says so.
    name: "identity-own-founded",
    path: `/identities/${ACME}`,
    ready: "ledger-events",
  },
  {
    // Every action starts closed (decision 017), so a screenshot of one opens it.
    name: "identity-own-push",
    path: `/identities/${ALICE}`,
    ready: "action-push-summary",
    async act(page) {
      await page.getByTestId("action-push-summary").click();
      await page.getByTestId("sync-push-submit").click();
      await page.getByTestId("sync-push-results").waitFor();
    },
  },
  {
    // The handle action: the line to add to DNS, and the check beside it.
    name: "identity-own-handle",
    path: `/identities/${ALICE}`,
    ready: "action-handle-summary",
    async act(page) {
      await page.getByTestId("action-handle-summary").click();
      await page.getByTestId("handle-form").waitFor();
    },
  },
  {
    // Taking back trust is a form naming the identity, not a row button.
    name: "identity-own-revoke",
    path: `/identities/${ALICE}`,
    ready: "action-revoke-summary",
    async act(page) {
      await page.getByTestId("action-revoke-summary").click();
      await page.getByTestId("trust-revoke-form").waitFor();
    },
  },
  {
    // The witnesses one identity chose, as cards with whole endpoint ids.
    name: "identity-own-witnesses",
    path: `/identities/${ALICE}`,
    ready: "action-witnesses-summary",
    async act(page) {
      await page.getByTestId("action-witnesses-summary").click();
      await page.getByTestId("witness-list").waitFor();
    },
  },
  {
    name: "identity-own-keys",
    path: `/identities/${ALICE}`,
    ready: "action-keys-summary",
    async act(page) {
      await page.getByTestId("action-keys-summary").click();
      await page.getByTestId("identity-keys-download").waitFor();
    },
  },
  {
    name: "identity-own-event",
    path: `/identities/${ALICE}`,
    ready: "ledger-events",
    async act(page) {
      await page.getByTestId("event-expand-2").click();
      await page.getByTestId("event-detail-2").waitFor();
    },
  },
  {
    name: "identity-own-invite",
    path: `/identities/${ALICE}`,
    ready: "action-invite-summary",
    async act(page) {
      await page.getByTestId("action-invite-summary").click();
      await page.getByTestId("invite-submit").waitFor();
    },
  },
  {
    // Not stored here and not held by any witness: the graph knows it, and the
    // page offers the one action a page like that has.
    name: "identity-foreign-unstored",
    path: `/identities/${CAROL}`,
    ready: "identity-fetch-button",
  },
  {
    // The two lists of "how you know them", opened: cards, not bespoke rows.
    name: "identity-foreign-expanded",
    path: `/identities/${CAROL}`,
    ready: "lookup-result",
    async act(page) {
      await page.getByTestId("lookup-trust-toggle").click();
      await page.getByTestId(`identity-card-${BOB}`).waitFor();
      await page.getByTestId("lookup-reverse-toggle").click();
    },
  },
  {
    // A record this home stored without controlling it: the ledger is there, the
    // actions are not, and the note and the crawl answer for the rest.
    name: "identity-foreign-stored",
    path: `/identities/${BOB}`,
    ready: "ledger-events",
  },
  {
    // The fetch, on a record one witness holds and this home does not: the page
    // that answered "no copy" becomes the page with the record on it.
    name: "identity-foreign-fetched",
    path: `/identities/${UNSTORED_LEDGER}`,
    ready: "identity-fetch-button",
    async act(page) {
      await page.getByTestId("identity-fetch-button").click();
      await page.getByTestId("ledger-events").waitFor();
    },
  },
  { name: "node", path: "/node", ready: "node-page" },
  { name: "witnesses", path: "/witnesses", ready: "graph-sync-button" },
  {
    // The sync consent, which moved off the header onto this page.
    name: "witnesses-sync-consent",
    path: "/witnesses",
    ready: "graph-sync-button",
    async act(page) {
      await page.getByTestId("graph-sync-button").click();
      await page.getByTestId("graph-sync-consent").waitFor();
    },
  },
  { name: "witness-ledgers", path: `/witnesses/${WITNESS}`, ready: "identity-cards" },
  {
    name: "witness-unreachable",
    path: `/witnesses/${UNREACHABLE_WITNESS}`,
    ready: "witness-unreachable",
  },
  { name: "witness-node-home", path: "/witness", ready: "identity-cards" },
  { name: "witness-node-ledger", path: `/witness/ledgers/${ALICE}`, ready: "ledger-events" },
];

/**
 * The page-level sideways scroll, with the elements that cause it: a box that
 * reaches past the viewport, or one whose own content does (a long id in a
 * sentence overflows its paragraph without widening the paragraph's box).
 */
function measureOverflow() {
  const root = document.documentElement;
  const limit = root.clientWidth;
  const offenders = [];
  for (const element of document.querySelectorAll("body *")) {
    const box = element.getBoundingClientRect();
    const past = box.right > limit + 1;
    const spills =
      getComputedStyle(element).overflowX === "visible" &&
      element.scrollWidth > element.clientWidth + 1;
    if (past || spills) {
      const testId = element.getAttribute("data-testid");
      offenders.push(
        `${element.tagName.toLowerCase()}${testId ? `[${testId}]` : ""} ` +
          `${past ? `right=${Math.round(box.right)}` : `content=${element.scrollWidth}px`}`,
      );
    }
  }
  return { scrollWidth: root.scrollWidth, clientWidth: limit, offenders: offenders.slice(0, 6) };
}

/**
 * Builds the harness and serves it, and answers where it is. Given a BASE_URL
 * the caller already runs something, so nothing is built and nothing is served.
 */
async function serveHarness() {
  if (process.env.BASE_URL) {
    return { baseUrl: process.env.BASE_URL, close: async () => {} };
  }
  await build({ configFile: HARNESS_CONFIG, logLevel: "warn" });
  const server = await preview({
    configFile: HARNESS_CONFIG,
    preview: { port: HARNESS_PORT, strictPort: true },
    logLevel: "warn",
  });
  return {
    baseUrl: `http://localhost:${HARNESS_PORT}`,
    close: () => server.close(),
  };
}

async function main() {
  await rm(OUT_DIR, { recursive: true, force: true });
  await mkdir(OUT_DIR, { recursive: true });

  const harness = await serveHarness();
  const BASE_URL = harness.baseUrl;
  const browser = await chromium.launch();
  const failures = [];
  try {
    for (const viewport of VIEWPORTS) {
      for (const screen of SCREENS) {
        // One context per screen: the mock store remembers what a visitor did in
        // localStorage, and a capture has to show the seeded state, not what the
        // capture before it left behind.
        const context = await browser.newContext({
          viewport: { width: viewport.width, height: viewport.height },
          deviceScaleFactor: 1,
          colorScheme: "light",
        });
        const page = await context.newPage();
        page.on("pageerror", (error) => failures.push(`${viewport.name}: ${error.message}`));
        await page.goto(`${BASE_URL}${screen.path}`, { waitUntil: "load" });
        await page.getByTestId(screen.ready).waitFor({ timeout: 15_000 });
        if (screen.act) {
          await screen.act(page);
        }
        await page.waitForTimeout(150);

        const overflow = await page.evaluate(measureOverflow);
        const scrolls = overflow.scrollWidth > overflow.clientWidth + 1;
        if (scrolls) {
          failures.push(
            `${screen.name} at ${viewport.name} scrolls sideways: ` +
              `${overflow.scrollWidth}px in ${overflow.clientWidth}px, ` +
              `from ${overflow.offenders.join(", ") || "an unnamed element"}`,
          );
        }
        // What a phone shows without scrolling, the bottom nav bar included.
        await page.screenshot({ path: `${OUT_DIR}/${screen.name}-${viewport.name}-top.png` });
        // The whole page. The nav is unpinned first: a fixed bar is painted over
        // the middle of a stitched full-page image, hiding a card.
        await page.addStyleTag({ content: "nav { position: static !important; }" });
        await page.screenshot({
          path: `${OUT_DIR}/${screen.name}-${viewport.name}.png`,
          fullPage: true,
        });
        console.log(`${scrolls ? "scrolls" : "ok     "} ${screen.name}-${viewport.name}.png`);
        await context.close();
      }
    }
  } finally {
    await browser.close();
    await harness.close();
  }

  if (failures.length > 0) {
    console.error(`\n${failures.length} problem(s):`);
    for (const failure of failures) {
      console.error(`  ${failure}`);
    }
    process.exitCode = 1;
    return;
  }
  console.log(
    `\n${SCREENS.length * VIEWPORTS.length} routes captured in ui/screenshots/, ` +
      "each as a full page and as a first screen (-top)",
  );
}

await main();
