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
import { Identifier } from "@/components/Identifier";
import { KeyValue, KeyValueTable } from "@/components/KeyValue";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { asApiError } from "@/hooks/useResource";

/**
 * The membership screens ticket 019 specified, rebuilt on the actions layout
 * and calling the ticket 021 routes. Every artifact crosses as base64 of the
 * bytes the CLI writes, and the node does all the signing: the browser holds no
 * keys (proposal 001 section 10).
 */

/** Who signs: the controllers this ledger records, the root first. */
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
      <Label htmlFor={testId}>by (the controller that signs)</Label>
      <select
        id={testId}
        data-testid={testId}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        className="h-10 w-full rounded-md border bg-transparent px-2 font-mono text-xs shadow-xs focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none"
      >
        {controllers.map((principal) => (
          <option key={principal.identity} value={principal.identity}>
            {principal.identity}
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
      <Label htmlFor={testId}>role</Label>
      <select
        id={testId}
        data-testid={testId}
        value={value}
        onChange={(event) => onChange(event.target.value as Role)}
        className="h-10 w-full rounded-md border bg-transparent px-2 text-sm shadow-xs focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none"
      >
        <option value="controller">controller, may append to the ledger</option>
        <option value="member">member, recorded with no signing authority</option>
      </select>
    </div>
  );
}

/** An artifact the person has to carry to the other wallet. */
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

/** Ticket 021's `POST .../memberships/invitations`. */
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
          label="invitee descriptor"
          testId="invite-descriptor"
          value={descriptor}
          onChange={setDescriptor}
          placeholder="base64 of the descriptor the invitee exported"
        />
        <Button type="submit" data-testid="invite-submit" disabled={pending}>
          {pending ? "appending" : "Invite"}
        </Button>
      </form>
      {error && <ErrorEnvelopeView error={error} testId="invite-error" />}
      {invited && (
        <Result testId="invite-result">
          <KeyValueTable>
            <KeyValue label="invitee" testId="invite-result-invitee">
              <Identifier value={invited.invitee} />
            </KeyValue>
            <KeyValue label="role" testId="invite-result-role">
              {invited.role}
            </KeyValue>
            <KeyValue label="invited at seq" testId="invite-result-seq">
              {invited.invitation_seq}
            </KeyValue>
          </KeyValueTable>
          <Artifact
            label="invitation bundle, for the invitee"
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
          label="invitation bundle"
          testId="accept-bundle"
          value={bundle}
          onChange={setBundle}
          placeholder="base64 of the bundle the inviter sent"
        />
        <Button type="submit" data-testid="accept-submit" disabled={pending}>
          {pending ? "reading" : "Read the invitation"}
        </Button>
      </form>
      {error && <ErrorEnvelopeView error={error} testId="accept-error" />}
      {accepted && (
        <Result testId="accept-result">
          <KeyValueTable>
            <KeyValue label="ledger" testId="accept-ledger-id">
              <Identifier value={accepted.ledger_id} />
            </KeyValue>
            <KeyValue label="declared kind" testId="accept-declared-kind">
              {accepted.declared_kind}
            </KeyValue>
            <KeyValue label="root" testId="accept-root">
              {accepted.root}
            </KeyValue>
            <KeyValue label="role offered" testId="accept-role">
              {accepted.role}
            </KeyValue>
            <KeyValue label="controllers" testId="accept-controllers">
              <span className="space-y-1">
                {accepted.controllers.map((controller) => (
                  <span key={controller.identity} className="block">
                    <Identifier value={controller.identity} />
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
              I understand, show the acceptance
            </Button>
          ) : (
            <Artifact
              label="acceptance, for a controller of that ledger"
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

/** Ticket 021's `POST .../memberships/admissions`. */
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
          label="acceptance"
          testId="admit-acceptance"
          value={acceptance}
          onChange={setAcceptance}
          placeholder="base64 of the acceptance the invitee sent back"
        />
        <Button type="submit" data-testid="admit-submit" disabled={pending}>
          {pending ? "appending" : "Admit"}
        </Button>
      </form>
      {error && <ErrorEnvelopeView error={error} testId="admit-error" />}
      {admitted && (
        <Result testId="admit-result">
          <KeyValueTable>
            <KeyValue label="admitted" testId="admit-result-invitee">
              <Identifier value={admitted.invitee} />
            </KeyValue>
            <KeyValue label="role" testId="admit-result-role">
              {admitted.role}
            </KeyValue>
            <KeyValue label="at seq" testId="admit-result-seq">
              {admitted.acceptance_seq}
            </KeyValue>
          </KeyValueTable>
        </Result>
      )}
    </div>
  );
}

/** Ticket 021's `POST .../memberships/removals`. */
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
          <Label htmlFor="remove-target">target</Label>
          <select
            id="remove-target"
            data-testid="remove-target"
            value={target}
            onChange={(event) => setTarget(event.target.value)}
            className="h-10 w-full rounded-md border bg-transparent px-2 font-mono text-xs shadow-xs focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none"
          >
            <option value="">choose a principal or an open invitation</option>
            {[...new Set(removable)].map((identityId) => (
              <option key={identityId} value={identityId}>
                {identityId}
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
          {pending ? "appending" : "Remove"}
        </Button>
      </form>
      {error && <ErrorEnvelopeView error={error} testId="remove-error" />}
      {removed && (
        <Result testId="remove-result">
          <KeyValueTable>
            <KeyValue label="target" testId="remove-result-target">
              <Identifier value={removed.target} />
            </KeyValue>
            <KeyValue label="principal removed" testId="remove-result-principal">
              {String(removed.principal_removed)}
            </KeyValue>
            <KeyValue label="invitation cancelled" testId="remove-result-invitation">
              <Identifier value={removed.invitation_cancelled} />
            </KeyValue>
            <KeyValue label="at seq" testId="remove-result-seq">
              {removed.removal_seq}
            </KeyValue>
          </KeyValueTable>
        </Result>
      )}
    </div>
  );
}
