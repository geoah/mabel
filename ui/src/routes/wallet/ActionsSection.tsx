import type { Identity, MembershipView } from "@/api/types";
import { Action } from "@/components/Action";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

import { ContactPanel } from "./ContactPanel";
import { AcceptForm, AdmitForm, InviteForm, RemoveForm } from "./MembershipForms";
import { ProfilePanel } from "./ProfilePanel";
import { SyncPushPanel } from "./SyncPushPanel";
import { type TrustActions, TrustAddForm } from "./TrustPanel";
import { VerificationPanel } from "./VerificationPanel";
import { WitnessConfigPanel } from "./WitnessConfigPanel";

/**
 * Everything this wallet can do to one identity, each operation named with one
 * line saying what it does (decision 014). The three a story runs on every
 * ledger open by default; the rest stay shut, because an address book page that
 * opens as twelve forms is not an address book page.
 */
export function ActionsSection({
  identity,
  memberships,
  trust,
  onAppended,
}: {
  identity: Identity;
  memberships: MembershipView | null;
  trust: TrustActions;
  onAppended: () => void;
}) {
  return (
    <Card data-testid="identity-actions">
      <CardHeader>
        <CardTitle>Actions</CardTitle>
        <CardDescription>
          Everything below appends to this ledger or writes this node home, except where it says
          otherwise
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-2">
        <Action
          testId="action-trust"
          title="Trust someone"
          description="Signs an attestation naming another identity as trusted by this one."
          defaultOpen
        >
          <TrustAddForm actions={trust} />
        </Action>
        <Action
          testId="action-revoke"
          title="Revoke trust"
          description="Withdraws one attestation; Revoke sits beside each row of the trust list above."
        >
          <p className="text-sm">
            A revocation names the attestation event it cancels and is appended to this ledger.
            Neither event ever leaves the chain, so a reader who sees the attestation also sees
            the revocation.
          </p>
        </Action>
        <Action
          testId="action-witnesses"
          title="Set the witnesses"
          description="Replaces the whole witness set on this ledger, 1 to 16 distinct endpoint ids."
          defaultOpen
        >
          <WitnessConfigPanel identity={identity} onAppended={onAppended} />
        </Action>
        <Action
          testId="action-push"
          title="Push to the witnesses"
          description="Sends this ledger's events to each configured witness and reports what each one did."
          defaultOpen
        >
          <SyncPushPanel identityId={identity.identity_id} />
        </Action>
        <Action
          testId="action-profile"
          title="Replace the profile"
          description="Publishes a display name and a hostname on the chain, replacing both at once."
        >
          <ProfilePanel identity={identity} onAppended={onAppended} />
        </Action>
        <Action
          testId="action-verification"
          title="Check the hostname"
          description="Queries DNS now for the hostname this profile claims; the verdict is advisory."
        >
          <VerificationPanel identity={identity} onChecked={onAppended} />
        </Action>
        <Action
          testId="action-contact"
          title="Edit the contact note"
          description="A private nickname and note kept in this node home, never signed and never synced."
        >
          <ContactPanel
            identityId={identity.identity_id}
            contact={identity.contact}
            onSaved={onAppended}
          />
        </Action>
        <Action
          testId="action-invite"
          title="Invite an identity"
          description="Appends an invitation for the identity in a descriptor file, and writes the bundle to hand back."
        >
          <InviteForm
            identity={identity}
            memberships={memberships}
            onAppended={onAppended}
          />
        </Action>
        <Action
          testId="action-accept"
          title="Accept an invitation"
          description="Reads a bundle addressed to this identity and has the node sign the acceptance."
        >
          <AcceptForm identity={identity} />
        </Action>
        <Action
          testId="action-admit"
          title="Admit an invitee"
          description="Appends a signed acceptance, which is what makes the invitee a principal here."
        >
          <AdmitForm identity={identity} memberships={memberships} onAppended={onAppended} />
        </Action>
        <Action
          testId="action-remove"
          title="Remove a principal"
          description="Takes a principal off this ledger, or cancels an invitation it has not accepted."
        >
          <RemoveForm identity={identity} memberships={memberships} onAppended={onAppended} />
        </Action>
        <Action
          testId="action-graph"
          title="Synchronize the trust graph"
          description="Crawls out from this node's identities so a foreign page can answer how you know someone."
        >
          <p className="text-sm">
            Synchronizing is manual and there is no timer. Sync graph sits in the header, beside
            the menu, with the counts of the crawl this home holds.
          </p>
        </Action>
      </CardContent>
    </Card>
  );
}
