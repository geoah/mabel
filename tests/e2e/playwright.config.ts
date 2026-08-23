import { defineConfig } from "@playwright/test";

/**
 * One worker, no parallelism: every story drives the one compose topology of
 * docker/compose.yaml, and story 005 continues from the containers story 004
 * started. Files run in name order, 001 to 007. Story 007 sorts last and
 * brings the topology up again with docker/compose.dns.yaml over it, which is
 * why global setup stays base-only.
 *
 * Timeouts are short on purpose (decision 010): everything here is local, so a
 * step that has not answered in ten seconds is a failure, not slow hardware.
 * A whole story gets two minutes, which covers its `docker compose up --wait`;
 * the two story 007 tests that wait out a query to a stopped resolver raise
 * their own.
 */
export default defineConfig({
  testDir: "./specs",
  fullyParallel: false,
  workers: 1,
  retries: 0,
  forbidOnly: !!process.env.CI,
  timeout: 120_000,
  expect: { timeout: 10_000 },
  reporter: [["list"]],
  globalSetup: require.resolve("./global-setup"),
  globalTeardown: require.resolve("./global-teardown"),
  use: {
    actionTimeout: 10_000,
    navigationTimeout: 10_000,
    trace: "retain-on-failure",
    video: "off",
    screenshot: "off",
  },
});
