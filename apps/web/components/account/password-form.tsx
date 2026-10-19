'use client';

import { useState, type FormEvent } from 'react';
import { Button, Card, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction } from '@/lib/api';
import { EMPTY_NEW_PASSWORD, NewPasswordFields, passwordsDiffer } from './new-password-fields';

/** Replaces the user's password; their other sessions are closed by the API. */
export function PasswordForm({ onChanged }: { onChanged?: () => void } = {}) {
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState(EMPTY_NEW_PASSWORD);
  const [done, setDone] = useState(false);
  const action = useAction();
  const mismatch = passwordsDiffer(next);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (mismatch) return;
    setDone(false);
    const changed = await action.run(() =>
      api.post('/account/password', { currentPassword: current, newPassword: next.password }),
    );
    if (changed) {
      setCurrent('');
      setNext(EMPTY_NEW_PASSWORD);
      setDone(true);
      onChanged?.();
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
        <NewPasswordFields value={next} onChange={setNext} label="Nouveau mot de passe" />
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
