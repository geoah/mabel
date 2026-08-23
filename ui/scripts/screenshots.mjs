#!/usr/bin/env node
// Captures every route of the demo build at three widths and reports any page
// that scrolls sideways. The images land in ui/screenshots/, which is ignored
// by git: they exist to be looked at, not to be committed.
//
//   VITE_DEMO=1 npx vite build --outDir dist-demo
//   npx vite preview --outDir dist-demo --port 4199 &
//   npm run screenshots
//
// BASE_URL overrides the server, which defaults to http://localhost:4199.

import { mkdir, rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";

import { chromium } from "playwright";

const BASE_URL = process.env.BASE_URL ?? "http://localhost:4199";
const OUT_DIR = fileURLToPath(new URL("../screenshots", import.meta.url));

/** The ids the demo fixtures carry; the same ones the component tests use. */
const ALICE = "sfttwjzd755ejzzantfeyylon5zhr7vjqrjywrulvbos77pcvuyq";
const BOB = "jwq7i3ex2my7stypeluecykconcej4ypwqmbisvxnbuhtus7jklq";
/** The foreign identity the lookup fixture answers for, and no witness holds. */
const CAROL = "jqtnsb2me7mj5xsze4gavqklohqhdmkshfiz65khjmxtxjruqh2q";
/** The two witness endpoints the demo knows: one answers, one does not. */
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
    name: "identity-own-push",
    path: `/identities/${ALICE}`,
    ready: "sync-push-submit",
    async act(page) {
      await page.getByTestId("sync-push-submit").click();
      await page.getByTestId("sync-push-results").waitFor();
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
    name: "identity-foreign-expanded",
    path: `/identities/${CAROL}`,
    ready: "lookup-result",
    async act(page) {
      await page.getByTestId(`lookup-trust-expand-${BOB}`).click();
      await page.getByTestId(`lookup-trust-expansion-${BOB}`).waitFor();
    },
  },
  {
    name: "identity-foreign-stored",
    path: `/identities/${BOB}`,
    ready: "identity-fetch-button",
    async act(page) {
      await page.getByTestId("identity-fetch-button").click();
      await page.getByTestId("ledger-events").waitFor();
    },
  },
  { name: "witnesses", path: "/witnesses", ready: "witness-cards" },
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

async function main() {
  await rm(OUT_DIR, { recursive: true, force: true });
  await mkdir(OUT_DIR, { recursive: true });

  const browser = await chromium.launch();
  const failures = [];
  try {
    for (const viewport of VIEWPORTS) {
      const context = await browser.newContext({
        viewport: { width: viewport.width, height: viewport.height },
        deviceScaleFactor: 1,
        colorScheme: "light",
      });
      const page = await context.newPage();
      page.on("pageerror", (error) => failures.push(`${viewport.name}: ${error.message}`));

      for (const screen of SCREENS) {
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
      }
      await context.close();
    }
  } finally {
    await browser.close();
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
