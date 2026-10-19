'use client';

import { useState, type FormEvent } from 'react';
import { Button, Card, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction } from '@/lib/api';
import { formatDate } from '@/lib/format';

const MIN_LENGTH = 12;

/** What a reset link opens, as returned by `/auth/password-reset/link`. */
export interface ResetView {
  email: string;
  expiresAt: string;
}

/** Chooses the new password; the API ends every other session and signs this one in. */
export function ResetPasswordForm({
  token,
  reset,
  onReset,
}: {
  token: string;
  reset: ResetView;
  onReset: () => void;
}) {
  const [password, setPassword] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const action = useAction();
  const mismatch = confirmation.length > 0 && confirmation !== password;

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (mismatch) return;
    const done = await action.run(() =>
      api.post('/auth/password-reset/complete', { token, password }),
    );
    if (done) onReset();
  }

  return (
    <Card className="p-6">
      <form onSubmit={submit} className="flex flex-col gap-4">
        <div className="text-sm">
          <p className="font-medium">Nouveau mot de passe</p>
          <p className="text-muted">
            Choisissez le nouveau mot de passe de {reset.email} (lien valable jusqu’au{' '}
            {formatDate(reset.expiresAt)}). Vos autres sessions seront fermées.
          </p>
        </div>
        {/* For password managers: the account the new password belongs to. */}
        <input type="hidden" autoComplete="username" value={reset.email} readOnly />
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
          Enregistrer et me connecter
        </Button>
      </form>
    </Card>
  );
}
