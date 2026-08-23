/**
 * Unix milliseconds rendered for a reader. Everything is printed in UTC: a
 * wallet, its node and a witness rarely share a timezone, and a screenshot or a
 * test that changes text with the machine's clock is worse than one that reads
 * an hour off.
 */
export function formatTimestamp(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms)) {
    return "null";
  }
  const iso = new Date(ms).toISOString();
  return `${iso.slice(0, 10)} ${iso.slice(11, 16)} UTC`;
}

/** The date alone, for a created row where the minute carries nothing. */
export function formatDate(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms)) {
    return "null";
  }
  return new Date(ms).toISOString().slice(0, 10);
}

const HOUR = 3_600_000;

/**
 * How long ago a timestamp was, in whole units, relative to a given now. It is
 * never precise: a crawl is stale after 24 hours and "2 days ago" is what the
 * reader has to act on.
 */
export function describeAge(ms: number, now: number = Date.now()): string {
  const elapsed = Math.max(0, now - ms);
  if (elapsed < HOUR) {
    const minutes = Math.floor(elapsed / 60_000);
    return minutes <= 1 ? "just now" : `${minutes} minutes ago`;
  }
  if (elapsed < 24 * HOUR) {
    const hours = Math.floor(elapsed / HOUR);
    return hours === 1 ? "1 hour ago" : `${hours} hours ago`;
  }
  const days = Math.floor(elapsed / (24 * HOUR));
  return days === 1 ? "1 day ago" : `${days} days ago`;
}
