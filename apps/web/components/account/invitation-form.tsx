'use client';

import { useState, type FormEvent } from 'react';
import { Button, Card, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction } from '@/lib/api';
import { formatDate } from '@/lib/format';

const MIN_LENGTH = 12;

/** What an invitation link opens, as returned by `/auth/invitation`. */
export interface InvitationView {
  email: string;
  displayName: string;
  company: string;
  expiresAt: string;
}

/** The invited person chooses a password; the API activates the account and signs it in. */
export function InvitationForm({
  token,
  invitation,
  onAccepted,
}: {
  token: string;
  invitation: InvitationView;
  onAccepted: () => void;
}) {
  const [password, setPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const action = useAction();
  const mismatch = confirmation.length > 0 && confirmation !== password;

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (mismatch) return;
    const accepted = await action.run(() =>
      api.post('/auth/invitation/accept', { token, password }),
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
        <Field label={`Mot de passe (${MIN_LENGTH} caractères minimum)`}>
          <Input
            type="password"
            required
            minLength={MIN_LENGTH}
            autoComplete="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </Field>
        <Field label="Confirmation">
          <Input
            type="password"
            required
            autoComplete="new-password"
            value={confirmation}
            onChange={(e) => setConfirmation(e.target.value)}
          />
        </Field>
        {mismatch && <ErrorNote message="Les deux saisies diffèrent." />}
        {action.error && <ErrorNote message={action.error} />}
        <Button type="submit" loading={action.pending} disabled={mismatch}>
          Activer mon compte
        </Button>
      </form>
    </Card>
  );
}
