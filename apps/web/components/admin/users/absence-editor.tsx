'use client';

import { useState } from 'react';
import { AbsenceForm } from '@/components/approvals';
import { Button, ErrorNote, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type AbsenceRecorded,
  type CollaboratorAbsence,
  type DirectoryUser,
} from '@/lib/api';
import { formatDate } from '@/lib/format';

/**
 * A collaborator's absence, declared or lifted for them by their account
 * administrator when they cannot (ADR-078): whom it names to stand in, who
 * declared it, whether the collaborator was told by e-mail.
 */
export function AbsenceEditor({ user }: { user: DirectoryUser }) {
  const path = `/admin/users/${user.id}/absence`;
  const state = useApiData<CollaboratorAbsence>(path);
  const [told, setTold] = useState<boolean | null>(null);
  const action = useAction();

  function recorded(answer: AbsenceRecorded) {
    setTold(answer.notified);
    state.reload();
  }

  async function end() {
    if (!window.confirm(`Lever l’absence de ${user.displayName} ?`)) return;
    const answer = await action.run(() => api.delete<AbsenceRecorded>(path));
    if (answer) recorded(answer);
  }

  return (
    <Loaded state={state}>
      {({ absence, candidates }) => (
        <div className="flex flex-col gap-3">
          {absence ? (
            <>
              <p className="text-sm">
                {absence.delegate.displayName} remplace {user.displayName} du{' '}
                {formatDate(absence.startsAt)} au {formatDate(absence.endsAt)}.{' '}
                {absence.declaredBy
                  ? `Absence déclarée par ${absence.declaredBy.displayName}.`
                  : 'Absence déclarée par la personne elle-même.'}
              </p>
              {action.error && <ErrorNote message={action.error} />}
              <Button variant="danger" className="w-fit" loading={action.pending} onClick={end}>
                Lever l’absence
              </Button>
            </>
          ) : (
            <>
              <p className="text-xs text-muted">
                Pour un collaborateur empêché de déclarer son absence lui-même : le remplaçant
                désigné valide à sa place les demandes qui l’attendent, là où il en a le droit ; le
                désigner ne lui en donne aucun. La personne est prévenue par e-mail, voit qui a
                déclaré son absence et peut y mettre fin.
              </p>
              <AbsenceForm
                candidates={candidates}
                submitLabel="Déclarer son absence"
                emptyNote="Aucun collègue ne détient le droit de valider des actions : personne ne peut remplacer ce collaborateur."
                declare={(body) => api.put<AbsenceRecorded>(path, body)}
                onDeclared={recorded}
              />
            </>
          )}
          {told !== null && (
            <p className="text-sm" role="status">
              {told
                ? `Un e-mail en informe ${user.displayName}.`
                : `Aucun e-mail n’a pu informer ${user.displayName} : prévenez cette personne.`}
            </p>
          )}
        </div>
      )}
    </Loaded>
  );
}
