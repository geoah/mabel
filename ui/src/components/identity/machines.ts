import type { Identity, WitnessSummary } from "@/api/types";

/**
 * One machine that answers for an identity, as a screen says it. `binding`,
 * `verified` and `hinted` are API words and stop here: what a reader gets is a
 * full sentence about where the claim came from (decision 017, proposal 006
 * section 8).
 */
export interface Machine {
  endpointId: string;
  /** True when the identity's own record lists it. */
  onOwnRecord: boolean;
}

/** The sentence for a machine the identity itself published. */
export const ON_OWN_RECORD = "This machine is listed on this identity's own record.";

/** The sentence for a machine nothing this home holds backs up. */
export const NOT_CONFIRMED = "No record we have confirms that this machine answers for it.";

/** Which of the two sentences one machine gets. */
export function machineSentence(machine: Machine): string {
  return machine.onOwnRecord ? ON_OWN_RECORD : NOT_CONFIRMED;
}

/**
 * Every machine this home would dial for one identity, in the order it would
 * try them: the ones the identity's own record publishes first, then the ones
 * this home only knows from somewhere else, which the witness list reports for
 * a witness identity.
 */
export function machinesOf(
  identity: Identity | null,
  witness?: WitnessSummary | null,
): Machine[] {
  const machines: Machine[] = (identity?.endpoints ?? []).map((endpointId) => ({
    endpointId,
    onOwnRecord: true,
  }));
  const listed = new Set(machines.map((machine) => machine.endpointId));
  for (const entry of witness?.endpoints ?? []) {
    if (listed.has(entry.endpoint_id)) {
      continue;
    }
    listed.add(entry.endpoint_id);
    machines.push({
      endpointId: entry.endpoint_id,
      onOwnRecord: entry.binding === "verified",
    });
  }
  return machines;
}
