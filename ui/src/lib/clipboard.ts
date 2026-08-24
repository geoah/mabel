/**
 * Copying text, in the two ways a browser offers, because the wallet is served
 * over plain http as often as not: `navigator.clipboard` exists only in a secure
 * context, and a node reached at `http://<host>:<port>` with `--allow-host` is
 * not one. The legacy textarea and `execCommand("copy")` path is what those
 * origins have, so it is not dead code here.
 *
 * Returns false when neither way worked, and the caller says so on screen: the
 * value is always visible beside the button, so the reader can select it.
 */
export async function copyText(value: string): Promise<boolean> {
  try {
    if (typeof navigator !== "undefined" && navigator.clipboard?.writeText) {
      await navigator.clipboard.writeText(value);
      return true;
    }
  } catch {
    // Denied permission, or a clipboard the browser refuses outside a gesture:
    // the legacy path below still works in both cases.
  }
  return legacyCopy(value);
}

/** What a reader is told when neither way worked. */
export const COPY_FAILED = "copy failed, select the text instead";

/**
 * The pre-clipboard-API copy: a textarea holding the value, selected, handed to
 * `document.execCommand("copy")`. jsdom and every non-browser host define no
 * execCommand at all, which reads here as "this way is not available".
 */
function legacyCopy(value: string): boolean {
  if (typeof document === "undefined" || typeof document.execCommand !== "function") {
    return false;
  }
  const area = document.createElement("textarea");
  area.value = value;
  area.setAttribute("readonly", "");
  // Off-screen but focusable: a display:none element cannot be selected.
  area.style.position = "fixed";
  area.style.top = "0";
  area.style.left = "-9999px";
  document.body.append(area);
  try {
    area.focus();
    area.select();
    area.setSelectionRange(0, value.length);
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    area.remove();
  }
}
