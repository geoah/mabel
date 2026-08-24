import type { Identity, MembershipView } from "@/api/types";
import { Action } from "@/components/Action";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";

import { ContactPanel } from "./ContactPanel";
import { HandlePanel } from "./HandlePanel";
import { KeysPanel } from "./KeysPanel";
import { AcceptForm, AdmitForm, InviteForm, RemoveForm } from "./MembershipForms";
import { ProfilePanel } from "./ProfilePanel";
import { SyncPushPanel } from "./SyncPushPanel";
import { type TrustActions, TrustAddForm, TrustRevokeForm } from "./TrustPanel";
import { WitnessConfigPanel } from "./WitnessConfigPanel";

/**
 * Everything this wallet can do to one identity, each named by the task it
 * performs for the person doing it (decision 017). Every one of them starts
 * closed: an address book page that opens as twelve forms is not an address
 * book page, and no single one of them is the thing a reader came for.
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
        <CardTitle>What you can do</CardTitle>
        <CardDescription>
          Everything here changes the public record, except the local info.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-2">
        <Action
          testId="action-trust"
          title="Say you trust someone"
          description="Their Mabel ID goes on this identity's public record."
        >
          <TrustAddForm actions={trust} />
        </Action>
        <Action
          testId="action-revoke"
          title="Take back trust"
          description="Name the identity you no longer trust. Both entries stay on the record."
        >
          <TrustRevokeForm identity={identity} actions={trust} />
        </Action>
        <Action
          testId="action-witnesses"
          title="Choose who keeps a copy"
          description="A witness keeps a copy of this record so other people can read it."
        >
          <WitnessConfigPanel identity={identity} onAppended={onAppended} />
        </Action>
        <Action
          testId="action-push"
          title="Send the record to the witnesses"
          description="Send this record to each witness you chose."
        >
          <SyncPushPanel identityId={identity.identity_id} />
        </Action>
        <Action
          testId="action-profile"
          title="Change the public name and email"
          description="Both are replaced together, and the old ones stay on the record."
        >
          <ProfilePanel identity={identity} onAppended={onAppended} />
        </Action>
        <Action
          testId="action-handle"
          title="Set the handle people can look you up by"
          description="A domain name that points at this identity in DNS."
        >
          <HandlePanel identity={identity} onAppended={onAppended} />
        </Action>
        <Action
          testId="action-keys"
          title="Save your keys"
          description="Copy or download the two secret keys that control this identity."
        >
          <KeysPanel identityId={identity.identity_id} />
        </Action>
        <Action
          testId="action-contact"
          title="Update local info"
          description="The nickname and note only this device sees."
        >
          <ContactPanel
            identityId={identity.identity_id}
            contact={identity.contact}
            onSaved={onAppended}
          />
        </Action>
        <Action
          testId="action-invite"
          title="Invite someone to help control this identity"
          description="You give them a file, they accept and send it back, and you confirm."
        >
          <InviteForm identity={identity} memberships={memberships} onAppended={onAppended} />
        </Action>
        <Action
          testId="action-accept"
          title="Accept an invitation someone sent you"
          description="Open the file they gave you, then send back the file this makes."
        >
          <AcceptForm identity={identity} />
        </Action>
        <Action
          testId="action-admit"
          title="Confirm someone you invited"
          description="Open the file they sent back. This is what puts them on the record."
        >
          <AdmitForm identity={identity} memberships={memberships} onAppended={onAppended} />
        </Action>
        <Action
          testId="action-remove"
          title="Remove someone"
          description="Take someone off this identity, or cancel an invitation they never accepted."
        >
          <RemoveForm identity={identity} memberships={memberships} onAppended={onAppended} />
        </Action>
      </CardContent>
    </Card>
  );
}
