'use client';

import { LinkPage, ResetPasswordForm, useLinkToken } from '@/components/account';
import type { ResetDetails } from '@/lib/api';

/** Opened from a password reset link (`/reset-password#<token>`), before any sign-in. */
export default function ResetPasswordPage() {
  const state = useLinkToken<ResetDetails>(
    '/auth/password-reset/link',
    'Lien impossible à vérifier',
  );
  return (
    <LinkPage state={state}>
      {(token, reset) => (
        <ResetPasswordForm
          token={token}
          reset={reset}
          onReset={() => window.location.assign('/chat')}
        />
      )}
    </LinkPage>
  );
}
