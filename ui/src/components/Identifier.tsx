import { useEffect, useState } from "react";
import { Link } from "react-router";

import { COPY_BUTTON } from "@/components/ui/icon-button";
import { COPY_FAILED, copyText } from "@/lib/clipboard";
import { MABEL_PREFIX } from "@/lib/link";
import { cn } from "@/lib/utils";

/** Characters kept visible at each end of a truncated identifier. */
export const HEAD_CHARS = 8;
export const TAIL_CHARS = 8;

/** How long a copy stays confirmed on the button that did it. */
const COPIED_MS = 2000;

/** What a copy button says it copies, when the caller does not name it. */
const COPY_LABEL = "Copy";

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
    <svg viewBox="0 0 16 16" aria-hidden="true" className="size-3" fill="currentColor">
      <path d="M6 1.5A1.5 1.5 0 0 0 4.5 3H4a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h8a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2h-.5A1.5 1.5 0 0 0 10 1.5H6Zm0 1h4a.5.5 0 0 1 .5.5v.5h-5V3a.5.5 0 0 1 .5-.5ZM4 4h.5A1.5 1.5 0 0 0 6 5.5h4A1.5 1.5 0 0 0 11.5 4H12a1 1 0 0 1 1 1v8a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1Z" />
    </svg>
  );
}

function CheckIcon() {
  return (
    <svg viewBox="0 0 16 16" aria-hidden="true" className="size-3" fill="currentColor">
      <path d="M13.78 4.22a.75.75 0 0 1 0 1.06l-6.5 6.5a.75.75 0 0 1-1.06 0l-3.5-3.5a.75.75 0 1 1 1.06-1.06l2.97 2.97 5.97-5.97a.75.75 0 0 1 1.06 0Z" />
    </svg>
  );
}

/**
 * Copies the identifier and reports it, including when it could not: a copy
 * nobody can see failing is worse than no copy button. The label names what is
 * copied, because "copy" alone tells a screen reader nothing about which of the
 * three ids on a card it would take. The confirmation holds for two seconds and
 * then goes, whatever the pointer does meanwhile.
 */
function CopyButton({ value, label }: { value: string; label: string }) {
  const [state, setState] = useState<"idle" | "copied" | "failed">("idle");
  const copied = state === "copied";
  const spoken = state === "failed" ? COPY_FAILED : copied ? `${label}: copied` : label;

  useEffect(() => {
    if (state === "idle") {
      return;
    }
    const timer = setTimeout(() => setState("idle"), COPIED_MS);
    return () => clearTimeout(timer);
  }, [state]);

  return (
    <>
      <button
        type="button"
        aria-label={spoken}
        title={spoken}
        data-copied={copied}
        data-copy-failed={state === "failed"}
        // A copy button inside a clickable card copies and nothing else, and it
        // sits above the card's own stretched link.
        onClick={(event) => {
          event.stopPropagation();
          void copyText(value).then((ok) => setState(ok ? "copied" : "failed"));
        }}
        className={cn(COPY_BUTTON, "relative z-10", copied && "text-foreground")}
      >
        {copied ? <CheckIcon /> : <ClipboardIcon />}
      </button>
      {state === "failed" && (
        <span data-testid="copy-failed" className="text-xs text-destructive">
          {COPY_FAILED}
        </span>
      )}
    </>
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
   * valid HTML.
   */
  plain?: boolean;
  /** Routes the value, for the ids that address a screen. */
  to?: string;
  /** The testid a suite reads on the link, when to is given. */
  linkTestId?: string;
  /** Makes the routed value cover its positioned ancestor: a card's click target. */
  stretch?: boolean;
  /** What the copy button says it copies: "Copy Mabel ID", "Copy Iroh ID". */
  copyLabel?: string;
  /**
   * Shows the value as `mabel://<id>`, which is how a Mabel identity id is put
   * in front of a person (decision 019). The prefix is display only: the copy
   * button takes the prefixed string, and `data-value` keeps the bare id that
   * an API path is built from. An Iroh endpoint id names a machine and never
   * sets this.
   */
  mabel?: boolean;
  className?: string;
}

/**
 * One identifier: a 52-character id, key, endpoint id or event id.
 *
 * A whole id stays whole, because it is the only thing telling two identities
 * apart, and it is drawn small, monospace and muted so it does not outshout the
 * name above it. The value and its copy button sit on one row, centred against
 * each other.
 *
 * The middle characters of a truncated value stay in the DOM inside an sr-only
 * span, so the element's text is always the whole value for a screen reader, a
 * test and a page copy, while a reader sees the first and last eight characters
 * with an ellipsis drawn by CSS.
 */
export function Identifier({
  value,
  full = false,
  plain = false,
  to,
  linkTestId,
  stretch = false,
  copyLabel = COPY_LABEL,
  mabel = false,
  className,
}: IdentifierProps) {
  const [expanded, setExpanded] = useState(false);

  if (value === null || value === undefined) {
    return <>null</>;
  }

  // The prefix is not part of the id, so the id alone is split: were the whole
  // shown string split, the eight characters a reader keeps would be `mabel://`
  // and none of the id under it.
  const prefix = mabel ? MABEL_PREFIX : "";
  const shown = `${prefix}${value}`;
  const parts = splitIdentifier(value);
  const whole = full || (expanded && !plain) || parts.middle === "";
  const body = whole ? (
    shown
  ) : (
    <>
      <span>
        {prefix}
        {parts.head}
      </span>
      <span className="sr-only">{parts.middle}</span>
      <span className="identifier-ellipsis">{parts.tail}</span>
    </>
  );

  return (
    <span
      data-value={value}
      data-truncated={String(!whole)}
      className={cn(
        // Small, monospace and muted: the id is evidence under a name, not the
        // heading of the card it sits on.
        "inline-flex max-w-full items-center gap-1 font-mono text-[11px] text-muted-foreground",
        whole ? "break-all" : "whitespace-nowrap",
        className,
      )}
    >
      {to !== undefined ? (
        <Link
          to={to}
          data-testid={linkTestId}
          title={shown}
          className={cn("min-w-0 underline", stretch && "after:absolute after:inset-0")}
        >
          {body}
        </Link>
      ) : full || plain ? (
        <span className="min-w-0" title={shown}>
          {body}
        </span>
      ) : (
        <button
          type="button"
          title={shown}
          aria-expanded={expanded}
          onClick={(event) => {
            event.stopPropagation();
            setExpanded(!expanded);
          }}
          className="relative z-10 min-w-0 text-left underline decoration-dotted decoration-from-font underline-offset-2 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          {body}
        </button>
      )}
      {!plain && <CopyButton value={shown} label={copyLabel} />}
    </span>
  );
}
