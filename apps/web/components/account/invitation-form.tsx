'use client';

import { useState, type FormEvent } from 'react';
import { Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type InvitationDetails, type LinkPassword } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { EMPTY_NEW_PASSWORD, NewPasswordFields, passwordsDiffer } from './new-password-fields';

/** The invited person chooses a password; the API activates the account and signs it in. */
export function InvitationForm({
  token,
  invitation,
  onAccepted,
}: {
  token: string;
  invitation: InvitationDetails;
  onAccepted: () => void;
}) {
  const [chosen, setChosen] = useState(EMPTY_NEW_PASSWORD);
  const action = useAction();
  const mismatch = passwordsDiffer(chosen);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (mismatch) return;
    const accepted = await action.run(() =>
      api.post('/auth/invitation/accept', {
        token,
        password: chosen.password,
      } satisfies LinkPassword),
    );
    if (accepted) onAccepted();
  }

  return (
    <Card className="p-6">
      <form onSubmit={submit} className="flex flex-col gap-4">
        <div className="text-sm">
          <p className="font-medium">Bienvenue, {invitation.displayName}</p>
          <p className="text-muted">
            {invitation.company} vous invite à utiliser votre collaborateur IA avec l’adresse{' '}
            {invitation.email}. Choisissez votre mot de passe pour activer votre compte (lien
            valable jusqu’au {formatDate(invitation.expiresAt)}).
          </p>
        </div>
        <NewPasswordFields value={chosen} onChange={setChosen} />
        {action.error && <ErrorNote message={action.error} />}
        <Button type="submit" loading={action.pending} disabled={mismatch}>
          Activer mon compte
        </Button>
      </form>
    </Card>
  );
}
