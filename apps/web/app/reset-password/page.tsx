'use client';

import { LinkPage, ResetPasswordForm, useLinkToken, type ResetView } from '@/components/account';

/** Opened from a password reset link (`/reset-password#<token>`), before any sign-in. */
export default function ResetPasswordPage() {
  const state = useLinkToken<ResetView>('/auth/password-reset/link', 'Lien impossible à vérifier');
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
