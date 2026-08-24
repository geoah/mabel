// The mabel link, built. Nothing here reads one: a link a person pastes goes to
// `GET /api/resolve?input=`, because the grammar lives in mabel-core and the
// browser gets no second implementation (proposal 006 section 7).

/**
 * How many machines a link carries. The payload allows eight; a link stops at
 * four, which keeps it at 282 characters, short enough for a chat message, a
 * printed line and a square someone can scan.
 */
export const MAX_LINK_MACHINES = 4;

/**
 * `mabel://<identity id>[?endpoints=<machine>[,<machine>]{0,3}]`, lowercase,
 * with no trailing slash. Machines past the fourth are left off rather than
 * making a link nothing will parse.
 */
export function mabelLink(identityId: string, machines: string[] = []): string {
  const carried = machines.slice(0, MAX_LINK_MACHINES);
  const id = identityId.toLowerCase();
  if (carried.length === 0) {
    return `mabel://${id}`;
  }
  return `mabel://${id}?endpoints=${carried.map((machine) => machine.toLowerCase()).join(",")}`;
}

/**
 * The file someone walks away with: one line, the link, UTF-8, a trailing
 * newline and no byte order mark.
 */
export function mabelLinkFile(link: string): string {
  return `${link}\n`;
}

/** What the download is called: short, and named after the identity it opens. */
export function mabelLinkFileName(identityId: string): string {
  return `${identityId.slice(0, 8)}.mabel`;
}
