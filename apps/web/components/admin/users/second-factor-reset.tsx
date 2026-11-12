'use client';

import { Button } from '@/components/ui';
import { api, type Action, type DirectoryUser, type Done } from '@/lib/api';
import type { AccountNotice } from './types';

/**
 * Resets the account's second factor (ADR-202), offered while it has one
 * on: for who lost its app and its recovery codes. Its sessions close; its
 * next sign-in with a password opens a session, or first an enrolment where
 * the company requires one.
 */
export function SecondFactorReset({
  user,
  action,
  onNotice,
}: {
  user: DirectoryUser;
  action: Action;
  onNotice: (notice: AccountNotice) => void;
}) {
  async function reset() {
    if (
      !window.confirm(
        `Réinitialiser le second facteur de ${user.displayName} ? Ses codes de secours ne serviront plus et ses sessions seront fermées.`,
      )
    )
      return;
    const done = await action.run(() => api.delete<Done>(`/admin/users/${user.id}/second-factor`));
    // The notice reads the directory again, as for the other credentials.
    if (done) onNotice({ kind: 'secondFactorReset', who: user.displayName });
  }

  return (
    <Button type="button" variant="secondary" disabled={action.pending} onClick={reset}>
      Réinitialiser le second facteur
    </Button>
  );
}
