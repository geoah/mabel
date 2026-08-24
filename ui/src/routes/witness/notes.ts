// The two standing notes of the witness route, kept in one place because the
// record list, the record detail and the conflict view all repeat them.

/**
 * Proposal 001 section 6, flag D: there is no global discovery and no "who
 * trusts B" query, so this list enumerates one witness's own store.
 */
export const WITNESS_HOLDINGS_NOTE =
  "This is what this one witness holds. A record missing here may still be on another witness.";

/**
 * Proposal 001 section 5, flag W. The wording is deliberate: a conflict record
 * carries evidence and attributes nothing to anyone.
 */
export const FORK_EVIDENCE_NOTE =
  "Two valid entries were signed at the same position by whoever held the key. " +
  "That can be deliberate or two controllers acting at once, and this record " +
  "proves nothing beyond the conflict.";

/** The witness signs nothing and stores nothing on request from this UI. */
export const WITNESS_READ_ONLY_NOTE = "This page only reads. Nothing here changes anything.";
