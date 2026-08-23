import { useState } from "react";
import { Link } from "react-router";

import { cn } from "@/lib/utils";

/** Characters kept visible at each end of a truncated identifier. */
export const HEAD_CHARS = 8;
export const TAIL_CHARS = 8;

export interface IdentifierParts {
  head: string;
  /** The characters hidden between head and tail; empty when nothing is hidden. */
  middle: string;
  tail: string;
}

/**
 * Splits an identifier into the head that stays visible, the middle that is
 * hidden, and the tail that stays visible. A value short enough to fit is
 * returned whole, with an empty middle and tail.
 */
export function splitIdentifier(
  value: string,
  head = HEAD_CHARS,
  tail = TAIL_CHARS,
): IdentifierParts {
  if (value.length <= head + tail + 1) {
    return { head: value, middle: "", tail: "" };
  }
  return {
    head: value.slice(0, head),
    middle: value.slice(head, value.length - tail),
    tail: value.slice(value.length - tail),
  };
}

/** The truncated form as a reader sees it, for tests and for tooling. */
export function middleTruncate(value: string, head = HEAD_CHARS, tail = TAIL_CHARS): string {
  const parts = splitIdentifier(value, head, tail);
  return parts.middle === "" ? parts.head : `${parts.head}…${parts.tail}`;
}

function ClipboardIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" className="size-3.5" fill="currentColor">
      <path d="M6 1.5A1.5 1.5 0 0 0 4.5 3H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2h-.5A1.5 1.5 0 0 0 10 1.5H6Zm0 1h4a.5.5 0 0 1 .5.5v.5h-5V3a.5.5 0 0 1 .5-.5ZM4 4h.5A1.5 1.5 0 0 0 6 5.5h4A1.5 1.5 0 0 0 11.5 4H12a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Z" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" className="size-3.5" fill="currentColor">
      <path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-6.5 6.5a.75.75 0 0 1-1.06 0l-3.5-3.5a.75.75 0 1 1 1.06-1.06l2.97 2.97 5.97-5.97a.75.75 0 0 1 1.06 0Z" />
    </svg>
  );
}

/**
 * Copies the identifier and reports it. The confirmation is held until the
 * button loses focus or the pointer leaves, so no timer fires after a render.
 */
function CopyButton({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard?.writeText(value);
    } catch {
      // No clipboard permission and no clipboard: report nothing.
      return;
    }
    setCopied(true);
  }

  return (
    <button
      type="button"
      aria-label={copied ? "copied" : "copy"}
      title={copied ? "copied" : "copy"}
      data-copied={copied}
      onClick={() => void copy()}
      onBlur={() => setCopied(false)}
      onPointerLeave={() => setCopied(false)}
      className={cn(
        "inline-flex size-10 shrink-0 items-center justify-center rounded-md md:size-6",
        "focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
        copied ? "text-foreground" : "text-muted-foreground hover:text-foreground",
        "hover:bg-accent",
      )}
    >
      {copied ? <CheckIcon /> : <ClipboardIcon />}
    </button>
  );
}

interface IdentifierProps {
  /** null and undefined render as the literal null the API document carries. */
  value: string | null | undefined;
  /** Renders the whole value, wrapped, with no toggle. */
  full?: boolean;
  /**
   * Renders the truncated value as text with no expand and no copy button. It
   * is what an identifier inside a link uses: a button inside an anchor is not
   * valid HTML, and the whole identity card is one anchor.
   */
  plain?: boolean;
  /** Routes the value, for the ids that address a screen. */
  to?: string;
  /** The testid a suite reads on the link, when to is given. */
  linkTestId?: string;
  className?: string;
}

/**
 * One identifier: a 52-character id, key, endpoint id or event id.
 *
 * The middle characters stay in the DOM inside an sr-only span, so the element's
 * text is always the whole value for a screen reader, a test and a page copy,
 * while a reader sees the first and last eight characters with an ellipsis drawn
 * by CSS. Clicking the value shows the rest; the title attribute carries it too.
 */
export function Identifier({
  value,
  full = false,
  plain = false,
  to,
  linkTestId,
  className,
}: IdentifierProps) {
  const [expanded, setExpanded] = useState(false);

  if (value === null || value === undefined) {
    return <>null</>;
  }

  const parts = splitIdentifier(value);
  const whole = full || (expanded && !plain) || parts.middle === "";
  const body = whole ? (
    value
  ) : (
    <>
      <span>{parts.head}</span>
      <span className="sr-only">{parts.middle}</span>
      <span className="identifier-ellipsis">{parts.tail}</span>
    </>
  );

  return (
    <span
      data-value={value}
      data-truncated={String(!whole)}
      className={cn(
        "inline-flex max-w-full gap-1 font-mono text-xs",
        // A truncated value is one short line and stays on it; a whole value
        // breaks wherever it must, because no column fits 52 characters.
        whole ? "items-start break-all" : "items-center whitespace-nowrap",
        className,
      )}
    >
      {to !== undefined ? (
        <Link to={to} data-testid={linkTestId} title={value} className="min-w-0 underline">
          {body}
        </Link>
      ) : full || plain ? (
        <span className="min-w-0" title={value}>
          {body}
        </span>
      ) : (
        <button
          type="button"
          title={value}
          aria-expanded={expanded}
          onClick={() => setExpanded(!expanded)}
          className="min-w-0 text-left underline decoration-dotted decoration-from-font underline-offset-2 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          {body}
        </button>
      )}
      {!plain && <CopyButton value={value} />}
    </span>
  );
}
