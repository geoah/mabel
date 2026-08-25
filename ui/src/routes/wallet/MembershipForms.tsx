import { type FormEvent, type ReactNode, useState } from "react";

import {
  acceptInvitation,
  admit,
  type ApiError,
  invite,
  removePrincipal,
} from "@/api/client";
import type {
  AcceptedResponse,
  AdmittedResponse,
  Identity,
  InvitedResponse,
  MembershipView,
  RemovedResponse,
  Role,
} from "@/api/types";
import { Base64Upload } from "@/components/Base64Upload";
import { ErrorEnvelopeView } from "@/components/ErrorEnvelopeView";
import { bareIdentity, IdentityInline } from "@/components/identity";
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { asApiError } from "@/hooks/useResource";
import { mabelId } from "@/lib/link";

/**
 * Bringing other people onto one identity: you invite them with a file, they
 * accept and hand a file back, and you confirm it. Every file crosses as base64
 * of the bytes the CLI writes, and the node does all the signing: the browser
 * holds no keys (proposal 001 section 10).
 */

/** Who signs: the controllers this identity records, the founder first. */
function SignerSelect({
  identity,
  memberships,
  value,
  onChange,
  testId,
}: {
  identity: Identity;
  memberships: MembershipView | null;
  value: string;
  onChange: (value: string) => void;
  testId: string;
}) {
  const principals = memberships?.principals ?? identity.principals;
  const controllers = principals.filter((principal) => principal.role === "controller");

  return (
    <div className="space-y-1">
      <Label htmlFor={testId}>Who signs</Label>
      <select
        id={testId}
        data-testid={testId}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="h-10 w-full rounded-md border bg-transparent px-2 font-mono text-xs shadow-xs focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none"
      >
        {/* The value stays the bare id the request carries; what a reader picks
            from reads as the link it is (decision 019). */}
        {controllers.map((principal) => (
          <option key={principal.identity} value={principal.identity}>
            {mabelId(principal.identity)}
          </option>
        ))}
      </select>
    </div>
  );
}

function RoleSelect({
  value,
  onChange,
  testId,
}: {
  value: Role;
  onChange: (value: Role) => void;
  testId: string;
}) {
  return (
    <div className="space-y-1">
      <Label htmlFor={testId}>What they may do</Label>
      <select
        id={testId}
        data-testid={testId}
        value={value}
        onChange={(event) => onChange(event.target.value as Role)}
        className="h-10 w-full rounded-md border bg-transparent px-2 text-sm shadow-xs focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none"
      >
        <option value="controller">controller: may act for this identity</option>
        <option value="member">member: listed here, may not act for it</option>
      </select>
    </div>
  );
}

/** A file the person has to carry to the other wallet. */
function Artifact({
  label,
  value,
  filename,
  testId,
}: {
  label: string;
  value: string;
  filename: string;
  testId: string;
}) {
  return (
    <div className="space-y-1">
      <Label htmlFor={testId}>{label}</Label>
      <textarea
        id={testId}
        data-testid={testId}
        readOnly
        value={value}
        rows={3}
        className="w-full rounded-md border bg-muted px-2 py-1 font-mono text-xs break-all"
      />
      <a
        href={`data:application/octet-stream;base64,${value}`}
        download={filename}
        data-testid={`${testId}-download`}
        className="inline-flex min-h-9 items-center text-sm underline"
      >
        Download {filename}
      </a>
    </div>
  );
}

function Result({ testId, children }: { testId: string; children: ReactNode }) {
  return (
    <div data-testid={testId} className="space-y-2 rounded-md border p-2">
      {children}
    </div>
  );
}

/** Inviting: appends the invitation and writes the file to hand over. */
export function InviteForm({
  identity,
  memberships,
  onAppended,
}: {
  identity: Identity;
  memberships: MembershipView | null;
  onAppended: () => void;
}) {
  const root = identity.principals.find((principal) => principal.is_root);
  const [by, setBy] = useState(root?.identity ?? identity.identity_id);
  const [role, setRole] = useState<Role>("controller");
  const [descriptor, setDescriptor] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [invited, setInvited] = useState<InvitedResponse | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    setInvited(null);
    try {
      setInvited(
        await invite(identity.identity_id, {
          by,
          role,
          invitee_descriptor_base64: descriptor.trim(),
        }),
      );
      onAppended();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <div className="space-y-3">
      <form onSubmit={submit} className="space-y-2" data-testid="invite-form">
        <SignerSelect
          identity={identity}
          memberships={memberships}
          value={by}
          onChange={setBy}
          testId="invite-by"
        />
        <RoleSelect value={role} onChange={setRole} testId="invite-role" />
        <Base64Upload
          label="Their identity file"
          testId="invite-descriptor"
          value={descriptor}
          onChange={setDescriptor}
          placeholder="paste the file they sent you, or pick it below"
        />
        <Button type="submit" data-testid="invite-submit" disabled={pending}>
          {pending ? "inviting" : "Invite"}
        </Button>
      </form>
      {error && <ErrorEnvelopeView error={error} testId="invite-error" />}
      {invited && (
        <Result testId="invite-result">
          <KeyValueTable>
            <KeyValue label="you invited" testId="invite-result-invitee">
              <IdentityInline
                identity={bareIdentity(invited.invitee)}
                to={`/identities/${invited.invitee}`}
              />
            </KeyValue>
            <KeyValue label="as" testId="invite-result-role">
              {invited.role}
            </KeyValue>
            <KeyValue label="recorded at position" testId="invite-result-seq">
              {invited.invitation_seq}
            </KeyValue>
          </KeyValueTable>
          <Artifact
            label="An invitation file to send them"
            value={invited.invitation_bundle_base64}
            filename="invitation.bundle"
            testId="invite-bundle"
          />
        </Result>
      )}
    </div>
  );
}

/**
 * Ticket 021's `POST .../memberships/acceptances`, called on the invitee's own
 * identity. Accepting a `controller` role on a raw-rooted ledger means signing
 * as that ledger's own identity, so the acceptance file stays hidden until the
 * person acknowledges that sentence (proposal 002 section 4).
 */
export function AcceptForm({ identity }: { identity: Identity }) {
  const [bundle, setBundle] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [accepted, setAccepted] = useState<AcceptedResponse | null>(null);
  const [acknowledged, setAcknowledged] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    setAccepted(null);
    setAcknowledged(false);
    try {
      setAccepted(
        await acceptInvitation(identity.identity_id, {
          invitation_bundle_base64: bundle.trim(),
        }),
      );
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  const warned = accepted?.controller_on_raw_root === true && !acknowledged;

  return (
    <div className="space-y-3">
      <form onSubmit={submit} className="space-y-2" data-testid="accept-form">
        <Base64Upload
          label="The invitation file they sent you"
          testId="accept-bundle"
          value={bundle}
          onChange={setBundle}
          placeholder="paste the file they sent you, or pick it below"
        />
        <Button type="submit" data-testid="accept-submit" disabled={pending}>
          {pending ? "reading" : "Read the invitation"}
        </Button>
      </form>
      {error && <ErrorEnvelopeView error={error} testId="accept-error" />}
      {accepted && (
        <Result testId="accept-result">
          <KeyValueTable>
            <KeyValue label="the identity inviting you" testId="accept-ledger-id">
              <IdentityInline
                identity={bareIdentity(accepted.ledger_id)}
                to={`/identities/${accepted.ledger_id}`}
              />
            </KeyValue>
            <KeyValue label="declared kind" testId="accept-declared-kind">
              {accepted.declared_kind}
            </KeyValue>
            <KeyValue label="how it signs" testId="accept-root">
              {accepted.root === "identity"
                ? "through its controllers, holding no key of its own"
                : "with a key of its own"}
            </KeyValue>
            <KeyValue label="you were offered" testId="accept-role">
              {accepted.role}
            </KeyValue>
            <KeyValue label="who controls it now" testId="accept-controllers">
              <span className="space-y-1">
                {accepted.controllers.map((controller) => (
                  <span key={controller.identity} className="block">
                    <IdentityInline
                      identity={bareIdentity(controller.identity)}
                      to={`/identities/${controller.identity}`}
                    />
                  </span>
                ))}
              </span>
            </KeyValue>
          </KeyValueTable>
          {accepted.warning !== null && (
            <p
              data-testid="accept-warning"
              className="rounded-md border border-destructive p-2 text-sm text-destructive"
            >
              {accepted.warning}
            </p>
          )}
          {warned ? (
            <Button
              size="sm"
              variant="destructive"
              data-testid="accept-acknowledge"
              onClick={() => setAcknowledged(true)}
            >
              I understand, show the file
            </Button>
          ) : (
            <Artifact
              label="A file to send back to whoever invited you"
              value={accepted.acceptance_base64}
              filename="acceptance.bin"
              testId="accept-acceptance"
            />
          )}
        </Result>
      )}
    </div>
  );
}

/** Confirming: reads the file the invitee handed back and records them. */
export function AdmitForm({
  identity,
  memberships,
  onAppended,
}: {
  identity: Identity;
  memberships: MembershipView | null;
  onAppended: () => void;
}) {
  const root = identity.principals.find((principal) => principal.is_root);
  const [by, setBy] = useState(root?.identity ?? identity.identity_id);
  const [acceptance, setAcceptance] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [admitted, setAdmitted] = useState<AdmittedResponse | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    setAdmitted(null);
    try {
      setAdmitted(
        await admit(identity.identity_id, { by, acceptance_base64: acceptance.trim() }),
      );
      onAppended();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <div className="space-y-3">
      <form onSubmit={submit} className="space-y-2" data-testid="admit-form">
        <SignerSelect
          identity={identity}
          memberships={memberships}
          value={by}
          onChange={setBy}
          testId="admit-by"
        />
        <Base64Upload
          label="The file they sent back"
          testId="admit-acceptance"
          value={acceptance}
          onChange={setAcceptance}
          placeholder="paste the file they sent back, or pick it below"
        />
        <Button type="submit" data-testid="admit-submit" disabled={pending}>
          {pending ? "confirming" : "Confirm"}
        </Button>
      </form>
      {error && <ErrorEnvelopeView error={error} testId="admit-error" />}
      {admitted && (
        <Result testId="admit-result">
          <KeyValueTable>
            <KeyValue label="you confirmed" testId="admit-result-invitee">
              <IdentityInline
                identity={bareIdentity(admitted.invitee)}
                to={`/identities/${admitted.invitee}`}
              />
            </KeyValue>
            <KeyValue label="as" testId="admit-result-role">
              {admitted.role}
            </KeyValue>
            <KeyValue label="recorded at position" testId="admit-result-seq">
              {admitted.acceptance_seq}
            </KeyValue>
          </KeyValueTable>
        </Result>
      )}
    </div>
  );
}

/** Removing: takes someone off the identity, or cancels their invitation. */
export function RemoveForm({
  identity,
  memberships,
  onAppended,
}: {
  identity: Identity;
  memberships: MembershipView | null;
  onAppended: () => void;
}) {
  const root = identity.principals.find((principal) => principal.is_root);
  const removable = [
    ...(memberships?.principals ?? identity.principals).map(
      (principal) => principal.identity,
    ),
    ...(memberships?.invitations ?? [])
      .filter((invitation) => invitation.status === "open")
      .map((invitation) => invitation.invitee),
  ];
  const [by, setBy] = useState(root?.identity ?? identity.identity_id);
  const [target, setTarget] = useState("");
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<ApiError | null>(null);
  const [removed, setRemoved] = useState<RemovedResponse | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setPending(true);
    setError(null);
    setRemoved(null);
    try {
      setRemoved(await removePrincipal(identity.identity_id, { by, target: target.trim() }));
      onAppended();
    } catch (thrown) {
      setError(asApiError(thrown));
    } finally {
      setPending(false);
    }
  }

  return (
    <div className="space-y-3">
      <form onSubmit={submit} className="space-y-2" data-testid="remove-form">
        <SignerSelect
          identity={identity}
          memberships={memberships}
          value={by}
          onChange={setBy}
          testId="remove-by"
        />
        <div className="space-y-1">
          <Label htmlFor="remove-target">Who to remove</Label>
          <select
            id="remove-target"
            data-testid="remove-target"
            value={target}
            onChange={(event) => setTarget(event.target.value)}
            className="h-10 w-full rounded-md border bg-transparent px-2 font-mono text-xs shadow-xs focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none"
          >
            <option value="">choose someone, or an invitation they never accepted</option>
            {[...new Set(removable)].map((identityId) => (
              <option key={identityId} value={identityId}>
                {mabelId(identityId)}
              </option>
            ))}
          </select>
        </div>
        <Button
          type="submit"
          variant="destructive"
          data-testid="remove-submit"
          disabled={pending || target === ""}
        >
          {pending ? "removing" : "Remove"}
        </Button>
      </form>
      {error && <ErrorEnvelopeView error={error} testId="remove-error" />}
      {removed && (
        <Result testId="remove-result">
          <KeyValueTable>
            <KeyValue label="removed" testId="remove-result-target">
              <IdentityInline
                identity={bareIdentity(removed.target)}
                to={`/identities/${removed.target}`}
              />
            </KeyValue>
            <KeyValue label="taken off this identity" testId="remove-result-principal">
              {removed.principal_removed ? "yes" : "no"}
            </KeyValue>
            <KeyValue label="invitation cancelled" testId="remove-result-invitation">
              {removed.invitation_cancelled === null ? (
                "none"
              ) : (
                <Identifier value={removed.invitation_cancelled} />
              )}
            </KeyValue>
            <KeyValue label="recorded at position" testId="remove-result-seq">
              {removed.removal_seq}
            </KeyValue>
          </KeyValueTable>
        </Result>
      )}
    </div>
  );
}
