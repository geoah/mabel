// The mabel link, built. The grammar lives in mabel-core and the browser gets no
// second implementation of it: a link a person pastes goes to
// `GET /api/resolve?input=` (proposal 006 section 7). The one thing read here is
// the prefix, so a box that takes one identity takes the id and the id with its
// prefix as the same input, and says so when a link brings more than that
// (decision 019).

/**
 * How many endpoints a link carries. The payload allows eight; a link stops at
 * four, which keeps it at 282 characters, short enough for a chat message, a
 * printed line and a square someone can scan.
 */
export const MAX_LINK_MACHINES = 4;

/**
 * What every identity id is shown with. A bare 52-character string tells a
 * reader nothing about what it names, so the prefix travels with it wherever a
 * person reads one (decision 019). Machine surfaces keep the bare id.
 */
export const MABEL_PREFIX = "mabel://";

/** One identity id as a person reads it: `mabel://<id>`. */
export function mabelId(identityId: string): string {
  return `${MABEL_PREFIX}${identityId.toLowerCase()}`;
}

/** The 52 characters an identity id is spelled in, in either case. */
const BARE_ID = /^[a-z2-7]{52}$/i;

/**
 * What a box that takes one identity says when a link brings endpoints with it.
 * They are not dropped and the box is not filled: a link that names where to
 * dial means something only where something dials.
 */
export const ENDPOINTS_NOT_ACCEPTED =
  "This box takes one identity. Paste the identity on its own, without the endpoints the link carries.";

/** What it says when the prefix is there and an identity is not. */
export const NOT_AN_IDENTITY = "This does not look like an identity.";

/**
 * The identity id a box that takes one identity was given.
 *
 * The id and the same id with its prefix are one input (decision 019). Anything
 * else that begins `mabel://` is refused here rather than sent on, because the
 * node would answer about a value this box cannot carry. A string with no
 * prefix is passed through untouched: the node owns what an identity id is, and
 * this never becomes a second opinion about it.
 */
export function identityIdInput(input: string): { id: string; error: null } | {
  id: null;
  error: string;
} {
  const typed = input.trim();
  const prefixed = typed.slice(0, MABEL_PREFIX.length).toLowerCase() === MABEL_PREFIX;
  if (!prefixed) {
    return { id: typed, error: null };
  }
  const rest = typed.slice(MABEL_PREFIX.length);
  if (BARE_ID.test(rest)) {
    return { id: rest.toLowerCase(), error: null };
  }
  return {
    id: null,
    error: rest.includes("?") ? ENDPOINTS_NOT_ACCEPTED : NOT_AN_IDENTITY,
  };
}

/**
 * `mabel://<identity id>[?endpoints=<endpoint>[,<endpoint>]{0,3}]`, lowercase,
 * with no trailing slash. Endpoints past the fourth are left off rather than
 * making a link nothing will parse.
 */
export function mabelLink(identityId: string, endpoints: string[] = []): string {
  const carried = endpoints.slice(0, MAX_LINK_MACHINES);
  const id = mabelId(identityId);
  if (carried.length === 0) {
    return id;
  }
  return `${id}?endpoints=${carried.map((endpoint) => endpoint.toLowerCase()).join(",")}`;
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
