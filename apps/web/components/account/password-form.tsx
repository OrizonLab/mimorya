'use client';

import { useState, type FormEvent } from 'react';
import { Button, Card, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction } from '@/lib/api';

const MIN_LENGTH = 12;

/** Replaces the user's password; their other sessions are closed by the API. */
export function PasswordForm() {
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const [confirmation, setConfirmation] = useState('');
  const [done, setDone] = useState(false);
  const action = useAction();
  const mismatch = confirmation.length > 0 && confirmation !== next;

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (mismatch) return;
    setDone(false);
    const changed = await action.run(() =>
      api.post('/account/password', { currentPassword: current, newPassword: next }),
    );
    if (changed) {
      setCurrent('');
      setNext('');
      setConfirmation('');
      setDone(true);
    }
  }

  return (
    <Card className="max-w-md p-4">
      <form onSubmit={submit} className="flex flex-col gap-3">
        <h2 className="text-sm font-medium">Changer mon mot de passe</h2>
        <Field label="Mot de passe actuel">
          <Input
            type="password"
            required
            autoComplete="current-password"
            value={current}
            onChange={(e) => setCurrent(e.target.value)}
          />
        </Field>
        <Field label={`Nouveau mot de passe (${MIN_LENGTH} caractères minimum)`}>
          <Input
            type="password"
            required
            minLength={MIN_LENGTH}
            autoComplete="new-password"
            value={next}
            onChange={(e) => setNext(e.target.value)}
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
        {mismatch && <ErrorNote message="Les deux saisies ne correspondent pas." />}
        {action.error && <ErrorNote message={action.error} />}
        {done && (
          <p role="status" className="text-sm text-ok">
            Mot de passe modifié. Vos autres sessions ont été fermées.
          </p>
        )}
        <Button type="submit" className="w-fit" loading={action.pending} disabled={mismatch}>
          Enregistrer
        </Button>
      </form>
    </Card>
  );
}
