import { composeUp, docker, mustRun, removeExtras, REPO_ROOT, run } from "./lib/docker";

/**
 * The image the whole suite runs is built from committed HEAD, through
 * `git archive`, so an edited working tree cannot change what the topology
 * serves halfway through a run. The commit is recorded as an image label, and
 * a rebuild is skipped when the label already matches.
 *
 * MABEL_E2E_REBUILD=1 forces the build; KEEP_TOPOLOGY=1 keeps the containers
 * after the run, for a failure worth poking at.
 */
export default async function globalSetup(): Promise<void> {
  const commit = mustRun("git", ["rev-parse", "HEAD"]).stdout.trim();
  const labelled = docker(["inspect", "-f", '{{index .Config.Labels "mabel.commit"}}', "mabel:dev"]);
  const current = labelled.status === 0 ? labelled.stdout.trim() : "";

  if (process.env.MABEL_E2E_REBUILD === "1" || current !== commit) {
    process.stdout.write(`e2e: building mabel:dev from ${commit}\n`);
    mustRun(
      "sh",
      [
        "-c",
        `git archive --format=tar HEAD | docker build -f docker/Dockerfile -t mabel:dev --label mabel.commit=${commit} -`,
      ],
      900_000,
    );
  } else {
    process.stdout.write(`e2e: reusing mabel:dev built from ${commit}\n`);
  }

  const version = run("docker", ["run", "--rm", "--entrypoint", "mabel", "mabel:dev", "--version"]);
  process.stdout.write(`e2e: image ${version.stdout.trim()} from ${REPO_ROOT}\n`);

  removeExtras();
  composeUp();
}
