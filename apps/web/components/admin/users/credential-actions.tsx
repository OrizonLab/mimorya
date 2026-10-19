'use client';

import { Button } from '@/components/ui';
import { api, type Action, type IssuedInvitation, type DirectoryUser } from '@/lib/api';
import type { AccountNotice } from './types';

/**
 * How an administrator gets an account (back) into the application: a new
 * invitation while it never chose its password; otherwise a reset link by
 * e-mail when a relay is configured (ADR-029), or a temporary password.
 * Disabled while a request runs: a second click would replace the link
 * just sent.
 */
export function CredentialActions({
  user,
  emailEnabled,
  action,
  onNotice,
}: {
  user: DirectoryUser;
  emailEnabled: boolean;
  action: Action;
  onNotice: (notice: AccountNotice) => void;
}) {
  async function resendInvitation() {
    if (
      !window.confirm(
        `Envoyer une nouvelle invitation à ${user.displayName} ? L’ancien lien ne fonctionnera plus.`,
      )
    )
      return;
    const issued = await action.run(() =>
      api.post<{ invitation: IssuedInvitation }>(`/admin/users/${user.id}/invitation`),
    );
    if (issued) {
      onNotice({
        kind: 'invitation',
        who: user.displayName,
        email: user.email ?? '',
        invitation: issued.invitation,
      });
    }
  }

  async function sendResetLink() {
    if (
      !window.confirm(
        `Envoyer à ${user.displayName} un lien pour choisir un nouveau mot de passe ? Son mot de passe actuel reste valable jusqu’à l’utilisation du lien.`,
      )
    )
      return;
    const sent = await action.run(() =>
      api.post<{ expiresAt: string }>(`/admin/users/${user.id}/password-link`),
    );
    if (sent) {
      onNotice({
        kind: 'resetLink',
        who: user.displayName,
        email: user.email ?? '',
        expiresAt: sent.expiresAt,
      });
    }
  }

  async function giveTemporaryPassword() {
    if (
      !window.confirm(
        `Remplacer le mot de passe de ${user.displayName} par un mot de passe provisoire ? Ses sessions seront fermées.`,
      )
    )
      return;
    const reset = await action.run(() =>
      api.post<{ temporaryPassword: string }>(`/admin/users/${user.id}/password`),
    );
    if (reset)
      onNotice({ kind: 'password', who: user.displayName, password: reset.temporaryPassword });
  }

  if (user.status === 'invited') {
    return (
      <Button
        type="button"
        variant="secondary"
        disabled={action.pending}
        onClick={resendInvitation}
      >
        Renvoyer l’invitation
      </Button>
    );
  }
  if (!emailEnabled || user.status !== 'active') {
    return (
      <Button
        type="button"
        variant="secondary"
        disabled={action.pending}
        onClick={giveTemporaryPassword}
      >
        Réinitialiser le mot de passe
      </Button>
    );
  }
  return (
    <>
      <Button type="button" variant="secondary" disabled={action.pending} onClick={sendResetLink}>
        Envoyer un lien de réinitialisation
      </Button>
      <Button
        type="button"
        variant="ghost"
        disabled={action.pending}
        onClick={giveTemporaryPassword}
      >
        Mot de passe provisoire
      </Button>
    </>
  );
}
