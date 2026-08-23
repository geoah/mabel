import { composeDown, removeExtras } from "./lib/docker";

/** KEEP_TOPOLOGY=1 leaves the containers and the homes up, for a post mortem. */
export default async function globalTeardown(): Promise<void> {
  if (process.env.KEEP_TOPOLOGY === "1") {
    process.stdout.write("e2e: KEEP_TOPOLOGY=1, leaving the topology up\n");
    return;
  }
  removeExtras();
  composeDown();
}
