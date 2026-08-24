import { type FormEvent, useState } from "react";

import { type ApiError, setContact } from "@/api/client";
import type { Contact } from "@/api/types";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { InfoTip, NICKNAME_INFO, NOTE_INFO } from "@/components/InfoTip";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { asApiError } from "@/hooks/useResource";
import { formatTimestamp } from "@/lib/time";

function trimmedOrNull(value: string): string | null {
  const trimmed = value.trim();
  return trimmed === "" ? null : trimmed;
}

/**
 * The two local fields this wallet keeps about an identity, its own or someone
 * else's: a nickname and a note. Neither is signed and neither is published, and
 * the nickname is one of the names a screen falls back to (proposal 003
 * section 1). One button writes both, because the contact route takes both.
 */
export function ContactPanel({
  identityId,
  contact,
  onSaved,
}: {
  identityId: string;
  contact: Contact | null;
  onSaved: () => void;
}) {
  const [nickname, setNickname] = useState(contact?.nickname ?? "");
  const [note, setNote] = useState(contact?.note ?? "");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [saved, setSaved] = useState<Contact | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    try {
      const response = await setContact(identityId, {
        nickname: trimmedOrNull(nickname),
        note: trimmedOrNull(note),
      });
      setSaved(response.contact);
      onSaved();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <div data-testid="contact-panel">
      <form onSubmit={submit} className="space-y-2" data-testid="contact-form">
        <div className="space-y-1">
          <div className="flex items-center gap-1">
            <Label htmlFor="contact-nickname">Nickname</Label>
            <InfoTip text={NICKNAME_INFO} testId="contact-nickname-info" />
          </div>
          <Input
            id="contact-nickname"
            data-testid="contact-nickname"
            value={nickname}
            onChange={(event) => setNickname(event.target.value)}
            placeholder="bob at the print shop"
          />
        </div>
        <div className="space-y-1">
          <div className="flex items-center gap-1">
            <Label htmlFor="contact-note">Note</Label>
            <InfoTip text={NOTE_INFO} testId="contact-note-info" />
          </div>
          <Input
            id="contact-note"
            data-testid="contact-note"
            value={note}
            onChange={(event) => setNote(event.target.value)}
            placeholder="met at the 2023 zine fair"
          />
        </div>
        <Button type="submit" size="sm" data-testid="contact-save" disabled={pending}>
          {pending ? "saving" : "Save"}
        </Button>
      </form>
      {error && <ErrorEnvelopeView error={error} testId="contact-error" />}
      {saved && (
        <p data-testid="contact-result" className="mt-2 text-xs">
          Saved {formatTimestamp(saved.updated_at_ms)}.
        </p>
      )}
    </div>
  );
}
