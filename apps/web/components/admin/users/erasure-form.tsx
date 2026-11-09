'use client';

import { useId, useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import {
  api,
  useAction,
  type AccountErasure,
  type DirectoryUser,
  type ErasedAccount,
} from '@/lib/api';

/**
 * The erasure of a disabled account (ADR-189), as its screen offers it to
 * who holds `user:erase` company-wide (ADR-190): the account's address
 * typed again confirms it, and the server's refusal (a run still living, a
 * request awaiting its decision, an absence it stands in for, a unit it
 * leads) is said as is.
 */
export function ErasureForm({
  user,
  onErased,
}: {
  user: DirectoryUser;
  onErased: (erased: ErasedAccount) => void;
}) {
  const [typed, setTyped] = useState('');
  const action = useAction();
  const explanation = useId();

  async function erase(event: FormEvent) {
    event.preventDefault();
    const erased = await action.run(() =>
      api.post<ErasedAccount>(`/admin/users/${user.id}/erasure`, {
        confirmEmail: typed,
      } satisfies AccountErasure),
    );
    if (erased) onErased(erased);
  }

  return (
    <form onSubmit={erase} className="flex flex-col gap-3">
      <p id={explanation} className="text-sm text-muted">
        L’effacement supprime le compte de {user.displayName} et ce que la plateforme garde sur
        cette personne : conversations, mémoires personnelles, tâches, demandes, comptes reliés. Ce
        qu’elle a laissé à l’entreprise reste, sans plus la nommer, et le journal d’audit garde les
        entrées qui la nomment. Il ne se défait pas.
      </p>
      <Field label="Adresse du compte, à retaper pour confirmer">
        <Input
          required
          type="email"
          autoComplete="off"
          aria-describedby={explanation}
          value={typed}
          onChange={(e) => setTyped(e.target.value)}
        />
      </Field>
      {action.error && <ErrorNote message={action.error} />}
      <Button type="submit" variant="danger" className="w-fit" loading={action.pending}>
        Effacer le compte
      </Button>
    </form>
  );
}
