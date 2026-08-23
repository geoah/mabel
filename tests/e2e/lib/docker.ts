import { spawnSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

/** The repository root, three levels above this file. */
export const REPO_ROOT = path.resolve(__dirname, "..", "..", "..");
export const COMPOSE_FILE = path.join(REPO_ROOT, "docker", "compose.yaml");
/** The test resolver overlay story 007 runs against (ticket 032). */
export const DNS_COMPOSE_FILE = path.join(REPO_ROOT, "docker", "compose.dns.yaml");

/** The image both roles run, and the compose project's derived names. */
export const IMAGE = "mabel:dev";
export const NETWORK = "mabel_mabel";
export const TICKET_VOLUME = "mabel_witness-ticket";

/** Host ports, which equal container ports because of the loopback rules. */
export const WITNESS_URL = "http://127.0.0.1:9080";
export const ALICE_URL = "http://127.0.0.1:9081";
export const BOB_URL = "http://127.0.0.1:9082";
export const WITNESS_TWO_URL = "http://127.0.0.1:9083";
export const ALICE_TWO_URL = "http://127.0.0.1:9084";

/** Containers and volumes stories 004, 005 and 006 start by hand. */
export const EXTRA_CONTAINERS = ["mabel-alice-two", "mabel-witness-two"];
export const EXTRA_VOLUMES = ["mabel-alice-second"];

export interface RunResult {
  status: number;
  stdout: string;
  stderr: string;
  command: string;
}

export function run(
  file: string,
  args: string[],
  timeoutMs = 60_000,
  env?: Record<string, string>,
): RunResult {
  const result = spawnSync(file, args, {
    cwd: REPO_ROOT,
    encoding: "utf8",
    timeout: timeoutMs,
    maxBuffer: 32 * 1024 * 1024,
    env: env ? { ...process.env, ...env } : process.env,
  });
  if (result.error) {
    throw new Error(`${file} ${args.join(" ")} failed to start: ${result.error.message}`);
  }
  return {
    status: result.status ?? -1,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
    command: `${file} ${args.join(" ")}`,
  };
}

/** Runs a command and throws its output when it exits non-zero. */
export function mustRun(
  file: string,
  args: string[],
  timeoutMs = 60_000,
  env?: Record<string, string>,
): RunResult {
  const result = run(file, args, timeoutMs, env);
  if (result.status !== 0) {
    throw new Error(
      `${result.command} exited ${result.status}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
    );
  }
  return result;
}

export function docker(args: string[], timeoutMs = 60_000): RunResult {
  return run("docker", args, timeoutMs);
}

export function dc(args: string[], timeoutMs = 120_000): RunResult {
  return run("docker", ["compose", "-f", COMPOSE_FILE, ...args], timeoutMs);
}

/** `dc exec -T <service> <command...>`. */
export function dcExec(service: string, args: string[], timeoutMs = 60_000): RunResult {
  return dc(["exec", "-T", service, ...args], timeoutMs);
}

/** `dc exec -T <service> mabel <args...>`. */
export function mabel(service: string, args: string[], timeoutMs = 60_000): RunResult {
  return dcExec(service, ["mabel", ...args], timeoutMs);
}

/** `dc exec -T <service> sh -c <script>`, for the `$(cat /shared/...)` forms. */
export function dcSh(service: string, script: string, timeoutMs = 60_000): RunResult {
  return dcExec(service, ["sh", "-c", script], timeoutMs);
}

/** `docker exec <container> sh -c <script>`, for the hand-started containers. */
export function dockerSh(container: string, script: string, timeoutMs = 60_000): RunResult {
  return docker(["exec", container, "sh", "-c", script], timeoutMs);
}

export function json<T = any>(result: RunResult): T {
  const text = result.stdout.trim();
  try {
    return JSON.parse(text) as T;
  } catch (error) {
    throw new Error(
      `${result.command} did not print JSON (exit ${result.status})\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
    );
  }
}

/** A throwaway container with an empty home, seeded with witness one's ticket. */
export function verifier(args: string[], timeoutMs = 60_000): RunResult {
  return docker(
    [
      "run",
      "--rm",
      "--network",
      NETWORK,
      "--volume",
      `${TICKET_VOLUME}:/shared:ro`,
      "--env",
      "MABEL_WAIT_FOR_TICKET=/shared/witness",
      IMAGE,
      ...args,
    ],
    timeoutMs,
  );
}

/** The lines the command printed on stdout, with the trailing blank dropped. */
export function stdoutLines(result: RunResult): string[] {
  return result.stdout.replace(/\n$/, "").split("\n");
}

/**
 * The base topology. `--remove-orphans` clears the story 007 resolver: it
 * belongs to this compose project, holds the network open and is not in this
 * file, so a run that ended before its teardown would block the network here.
 */
export function composeUp(): void {
  mustRun(
    "docker",
    ["compose", "-f", COMPOSE_FILE, "up", "-d", "--wait", "--remove-orphans"],
    180_000,
  );
}

/**
 * `down -v` naming both compose files, so a run that brought the story 007
 * overlay up leaves no resolver container, no `resolver-zones` volume and no
 * network behind for the next story to trip over. The overlay declares the
 * network's subnet, so a leftover network would also be the wrong one.
 *
 * A teardown that fails must fail the run: a half-removed topology is what
 * poisons the next story, and a story that starts on one is debugged from the
 * wrong end.
 */
export function composeDown(): void {
  mustRun(
    "docker",
    [
      "compose",
      "-f",
      COMPOSE_FILE,
      "-f",
      DNS_COMPOSE_FILE,
      "down",
      "-v",
      "--remove-orphans",
    ],
    120_000,
  );
}

/**
 * The topology of story 007: the base compose file plus the test resolver
 * overlay, brought up in two phases and answering with the witness's endpoint
 * id.
 *
 * Two phases because the wallets need `MABEL_WITNESSES`, and a witness's
 * endpoint id only exists once the witness has started. The witness and the
 * resolver come up first, `/shared/witness.id` is read, and the wallets start
 * with that id in their environment; compose interpolates it into the
 * overlay's `MABEL_WITNESSES`, the entrypoint runs `mabel witness set-default`
 * with it, and the crawler's third source has somewhere to ask.
 *
 * No `--build`: both images are the ones global-setup built from committed
 * HEAD, and `up --build` would rebuild them from the working tree.
 */
export function composeUpWithResolver(): string {
  const files = ["-f", COMPOSE_FILE, "-f", DNS_COMPOSE_FILE];
  mustRun(
    "docker",
    ["compose", ...files, "up", "-d", "--wait", "witness", "resolver"],
    300_000,
  );
  const witness = mustRun("docker", [
    "compose",
    ...files,
    "exec",
    "-T",
    "witness",
    "cat",
    "/shared/witness.id",
  ]).stdout.trim();
  mustRun("docker", ["compose", ...files, "up", "-d", "--wait"], 180_000, {
    MABEL_WITNESSES: witness,
  });
  return witness;
}

/** Removes the containers and volumes stories 004 to 006 start by hand. */
export function removeExtras(): void {
  docker(["rm", "-f", ...EXTRA_CONTAINERS], 60_000);
  docker(["volume", "rm", "-f", ...EXTRA_VOLUMES], 60_000);
}

/**
 * Story 001 step 1, which every story that starts from nothing repeats:
 * `dc down -v && dc up -d --wait`, with the hand-started containers cleared
 * first because they hold the network and the ticket volume open.
 */
export function resetTopology(): void {
  removeExtras();
  composeDown();
  composeUp();
}

/**
 * The same reset with the test resolver overlay, for story 007, answering
 * with the witness's endpoint id.
 */
export function resetTopologyWithResolver(): string {
  removeExtras();
  composeDown();
  return composeUpWithResolver();
}

export function containerRunning(name: string): boolean {
  const result = docker(["inspect", "-f", "{{.State.Running}}", name], 30_000);
  return result.status === 0 && result.stdout.trim() === "true";
}

/** The witness's endpoint id, as story 001 step 1 reads it. */
export function witnessId(): string {
  return mustRun("docker", [
    "compose",
    "-f",
    COMPOSE_FILE,
    "exec",
    "-T",
    "witness",
    "cat",
    "/shared/witness.id",
  ]).stdout.trim();
}

/** A file carried between two homes that share no disk, by `docker cp`. */
export function carry(
  fromContainer: string,
  fromPath: string,
  toContainer: string,
  toPath: string,
): void {
  const staging = fs.mkdtempSync(path.join(os.tmpdir(), "mabel-e2e-"));
  const hostPath = path.join(staging, path.basename(fromPath));
  try {
    mustRun("docker", ["cp", `${fromContainer}:${fromPath}`, hostPath]);
    mustRun("docker", ["cp", hostPath, `${toContainer}:${toPath}`]);
  } finally {
    fs.rmSync(staging, { recursive: true, force: true });
  }
}

/** Reads a file out of a container as a base64 string, without a host path. */
export function readFileBase64(container: string, filePath: string): string {
  return mustRun("docker", ["exec", container, "base64", "-w0", filePath]).stdout.trim();
}

/** Writes base64 bytes into a container as a file. */
export function writeFileBase64(container: string, filePath: string, base64: string): void {
  const staging = fs.mkdtempSync(path.join(os.tmpdir(), "mabel-e2e-"));
  const hostPath = path.join(staging, path.basename(filePath));
  try {
    fs.writeFileSync(hostPath, Buffer.from(base64, "base64"));
    mustRun("docker", ["cp", hostPath, `${container}:${filePath}`]);
  } finally {
    fs.rmSync(staging, { recursive: true, force: true });
  }
}

export async function apiGet<T = any>(base: string, route: string): Promise<{ status: number; body: T }> {
  const response = await fetch(`${base}${route}`);
  return { status: response.status, body: (await response.json()) as T };
}

export async function apiPost<T = any>(
  base: string,
  route: string,
  body: unknown,
): Promise<{ status: number; body: T }> {
  const response = await fetch(`${base}${route}`, {
    method: "POST",
    headers: { "Content-Type": "application/json", Origin: base },
    body: JSON.stringify(body),
  });
  return { status: response.status, body: (await response.json()) as T };
}

export async function apiPut<T = any>(
  base: string,
  route: string,
  body: unknown,
): Promise<{ status: number; body: T }> {
  const response = await fetch(`${base}${route}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json", Origin: base },
    body: JSON.stringify(body),
  });
  return { status: response.status, body: (await response.json()) as T };
}

/** Polls until the predicate holds, with a short ceiling: everything is local. */
export async function until(
  what: string,
  predicate: () => boolean | Promise<boolean>,
  timeoutMs = 30_000,
  intervalMs = 500,
): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    if (await predicate()) {
      return;
    }
    if (Date.now() > deadline) {
      throw new Error(`timed out after ${timeoutMs}ms waiting for ${what}`);
    }
    await new Promise((resolve) => setTimeout(resolve, intervalMs));
  }
}

/** Waits for a node's API to answer, the wait stories 004 and 006 spell out. */
export async function waitForNode(base: string, timeoutMs = 60_000): Promise<void> {
  await until(
    `${base}/api/node`,
    async () => {
      try {
        const response = await fetch(`${base}/api/node`);
        return response.ok;
      } catch {
        return false;
      }
    },
    timeoutMs,
  );
}
