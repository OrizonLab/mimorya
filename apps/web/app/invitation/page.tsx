'use client';

import { InvitationForm, LinkPage, useLinkToken, type InvitationView } from '@/components/account';

/** Opened from an invitation link (`/invitation#<token>`), before any sign-in. */
export default function InvitationPage() {
  const state = useLinkToken<InvitationView>(
    '/auth/invitation',
    'Invitation impossible à vérifier',
  );
  return (
    <LinkPage state={state}>
      {(token, invitation) => (
        <InvitationForm
          token={token}
          invitation={invitation}
          onAccepted={() => window.location.assign('/chat')}
        />
      )}
    </LinkPage>
  );
}
