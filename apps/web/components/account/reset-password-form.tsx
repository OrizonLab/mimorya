'use client';

import { useState, type FormEvent } from 'react';
import { Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { EMPTY_NEW_PASSWORD, NewPasswordFields, passwordsDiffer } from './new-password-fields';

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
  const [chosen, setChosen] = useState(EMPTY_NEW_PASSWORD);
  const action = useAction();
  const mismatch = passwordsDiffer(chosen);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (mismatch) return;
    const done = await action.run(() =>
      api.post('/auth/password-reset/complete', { token, password: chosen.password }),
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
        <NewPasswordFields value={chosen} onChange={setChosen} />
        {action.error && <ErrorNote message={action.error} />}
        <Button type="submit" loading={action.pending} disabled={mismatch}>
          Enregistrer et me connecter
        </Button>
      </form>
    </Card>
  );
}
