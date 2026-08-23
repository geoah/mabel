import { composeUp, docker, mustRun, removeExtras, REPO_ROOT, run } from "./lib/docker";

/**
 * Every image the suite runs is built from committed HEAD, through
 * `git archive`, so an edited working tree cannot change what the topology
 * serves halfway through a run. The commit is recorded as an image label, and
 * a rebuild is skipped when the label already matches.
 *
 * MABEL_E2E_COMMIT names the commit to build, HEAD by default; set it when
 * HEAD does not compile. MABEL_E2E_REBUILD=1 forces the build, and
 * KEEP_TOPOLOGY=1 keeps the containers after the run, for a post mortem.
 */
export default async function globalSetup(): Promise<void> {
  const revision = process.env.MABEL_E2E_COMMIT ?? "HEAD";
  const commit = mustRun("git", ["rev-parse", revision]).stdout.trim();

  // The node image, built from the whole tree.
  build("mabel:dev", commit, `git archive --format=tar ${commit}`, "docker/Dockerfile");
  // The story 007 test resolver. `<commit>:docker` archives that subtree with
  // its paths relative to it, which is the build context `compose.dns.yaml`
  // gives the same Dockerfile.
  build(
    "mabel-resolver:dev",
    commit,
    `git archive --format=tar ${commit}:docker`,
    "Dockerfile.resolver",
  );

  const version = run("docker", ["run", "--rm", "--entrypoint", "mabel", "mabel:dev", "--version"]);
  process.stdout.write(`e2e: image ${version.stdout.trim()} from ${REPO_ROOT}\n`);

  removeExtras();
  composeUp();
}

/** Builds one image from an archive of `commit`, unless its label matches. */
function build(image: string, commit: string, archive: string, dockerfile: string): void {
  const labelled = docker(["inspect", "-f", '{{index .Config.Labels "mabel.commit"}}', image]);
  const current = labelled.status === 0 ? labelled.stdout.trim() : "";
  if (process.env.MABEL_E2E_REBUILD !== "1" && current === commit) {
    process.stdout.write(`e2e: reusing ${image} built from ${commit}\n`);
    return;
  }
  process.stdout.write(`e2e: building ${image} from ${commit}\n`);
  mustRun(
    "sh",
    [
      "-c",
      `${archive} | docker build -f ${dockerfile} -t ${image} --label mabel.commit=${commit} -`,
    ],
    900_000,
  );
}
