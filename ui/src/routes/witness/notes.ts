// The two standing notes of the witness route, kept in one place because the
// ledger list, the ledger detail and the fork view all repeat them.

/**
 * Proposal 001 section 6, flag D: there is no global discovery and no "who
 * trusts B" query, so this list enumerates one witness's own store.
 */
export const WITNESS_HOLDINGS_NOTE =
  "this is what this one witness holds, a diagnostic and not an index: a ledger " +
  "missing here may still exist on another witness";

/**
 * Proposal 001 section 5, flag W. The wording is deliberate: a fork record
 * carries evidence and attributes nothing to anyone.
 */
export const FORK_EVIDENCE_NOTE =
  "a fork record proves two distinct validly signed events exist at one sequence, " +
  "produced by whoever held signing authority there: it is evidence of equivocation " +
  "or of a lost race between honest controllers, and it authorizes nothing";

/** The witness signs nothing and stores nothing on request from this UI. */
export const WITNESS_READ_ONLY_NOTE = "every request this route issues is a read";
