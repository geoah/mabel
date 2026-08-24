import type { Identity } from "@/api/types";
import { Identifier } from "@/components/Identifier";
import { type Machine } from "@/components/identity";
import { QrSquare } from "@/components/QrSquare";
import { mabelLink, mabelLinkFile, mabelLinkFileName, MAX_LINK_MACHINES } from "@/lib/link";

/**
 * What handing the link over gives away, said on the panel that makes one
 * (proposal 006 section 7). Three facts, no hedging.
 */
export const SHARE_DISCLOSURE = [
  "The link carries this identity's Mabel ID, which anyone holding it can read.",
  "It carries the machines that answer for this identity, so whoever has it can dial them directly.",
  "Whoever uses it asks those machines for this record, which tells them this home's network address.",
];

/**
 * The link that opens this identity somewhere else: the string with a copy
 * control, the same string as a square, and a file to hand over. The wallet
 * builds the string and never reads one back: a link a person pastes goes to
 * the node, which owns the grammar.
 */
export function SharePanel({
  identity,
  machines,
}: {
  identity: Identity;
  /** The machines the link names, capped at four. */
  machines: Machine[];
}) {
  const carried = machines.slice(0, MAX_LINK_MACHINES).map((machine) => machine.endpointId);
  const link = mabelLink(identity.identity_id, carried);
  const file = `data:text/plain;charset=utf-8,${encodeURIComponent(mabelLinkFile(link))}`;

  return (
    <div data-testid="share-panel" className="space-y-3">
      <Identifier value={link} full copyLabel="Copy the link" className="text-xs" />
      <p data-testid="share-machine-count" className="text-xs text-muted-foreground">
        {carried.length === 0
          ? "No machine answers for this identity yet, so the link carries the Mabel ID alone."
          : machines.length > carried.length
            ? `The link names the first ${carried.length} of the ${machines.length} machines that answer for this identity, which is as many as a link holds.`
            : `The link names ${carried.length} ${carried.length === 1 ? "machine" : "machines"}.`}
      </p>
      <QrSquare
        value={link}
        testId="share-qr"
        label={`A square holding the link to ${identity.identity_id}`}
      />
      <a
        href={file}
        download={mabelLinkFileName(identity.identity_id)}
        data-testid="share-download"
        className="inline-flex min-h-9 items-center text-sm underline"
      >
        Download the link as a file
      </a>
      <div data-testid="share-disclosure" className="space-y-1 border-t pt-3">
        {SHARE_DISCLOSURE.map((sentence) => (
          <p key={sentence} className="text-xs">
            {sentence}
          </p>
        ))}
      </div>
    </div>
  );
}
