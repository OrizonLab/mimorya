'use client';

import { InvitationForm, LinkPage, useLinkToken } from '@/components/account';
import { pathAfterSignIn } from '@/lib/sign-in';
import type { InvitationDetails } from '@/lib/api';

/** Opened from an invitation link (`/invitation#<token>`), before any sign-in. */
export default function InvitationPage() {
  const state = useLinkToken<InvitationDetails>(
    '/auth/invitation',
    'Invitation impossible à vérifier',
  );
  return (
    <LinkPage state={state}>
      {(token, invitation) => (
        <InvitationForm
          token={token}
          invitation={invitation}
          onAccepted={(answer) => window.location.assign(pathAfterSignIn(answer))}
        />
      )}
    </LinkPage>
  );
}
