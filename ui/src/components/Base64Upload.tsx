import type { ChangeEvent } from "react";

import { Label } from "@/components/ui/label";

/**
 * One of the files people hand each other to join an identity, crossing as
 * base64 of the bytes the CLI writes (contracts/README.md, "Artifacts over
 * JSON"). The wallet never parses one; it reads the file the person picked and
 * posts its bytes.
 *
 * The box is beside the picker on purpose: a file that arrived in a chat message
 * is pasted, one that arrived as a file is picked, and both end up as the same
 * string.
 */
export function Base64Upload({
  label,
  testId,
  value,
  onChange,
  placeholder,
}: {
  label: string;
  testId: string;
  value: string;
  onChange: (value: string) => void;
  placeholder?: string;
}) {
  async function read(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    if (!file) {
      return;
    }
    const bytes = new Uint8Array(await file.arrayBuffer());
    let binary = "";
    for (const byte of bytes) {
      binary += String.fromCharCode(byte);
    }
    onChange(btoa(binary));
  }

  return (
    <div className="space-y-1">
      <Label htmlFor={testId}>{label}</Label>
      <textarea
        id={testId}
        data-testid={testId}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder={placeholder ?? "paste the file's contents, or pick it below"}
        rows={3}
        className="w-full rounded-md border bg-transparent px-2 py-1 font-mono text-xs break-all shadow-xs focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none"
      />
      <input
        type="file"
        data-testid={`${testId}-file`}
        aria-label={`${label} file`}
        onChange={(event) => void read(event)}
        className="block w-full text-xs file:mr-2 file:min-h-8 file:rounded-md file:border file:bg-transparent file:px-2 file:text-xs"
      />
    </div>
  );
}
