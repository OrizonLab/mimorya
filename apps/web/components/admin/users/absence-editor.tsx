'use client';

import { useState } from 'react';
import { AbsenceForm } from '@/components/approvals';
import { Button, ErrorNote, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type Absence,
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
export function AbsenceEditor({
  user,
  emailEnabled,
}: {
  user: DirectoryUser;
  /** Whether this server sends e-mail at all. */
  emailEnabled: boolean;
}) {
  const path = `/admin/users/${user.id}/absence`;
  const state = useApiData<CollaboratorAbsence>(path);
  // What the last change told the collaborator; none until one is made.
  const [told, setTold] = useState<boolean | null>(null);
  const action = useAction();

  function recorded(answer: AbsenceRecorded) {
    setTold(answer.notified);
    state.reload();
  }

  async function declare(body: Absence) {
    setTold(null);
    return api.put<AbsenceRecorded>(path, body);
  }

  async function end() {
    if (!window.confirm(`Lever l’absence de ${user.displayName} ?`)) return;
    setTold(null);
    const answer = await action.run(() => api.delete<AbsenceRecorded>(path));
    // Refused, it may have ended meanwhile: shown as it now stands.
    if (answer) recorded(answer);
    else state.reload();
  }

  return (
    <Loaded state={state}>
      {({ absence, candidates }) => (
        <div className="flex flex-col gap-3">
          {absence ? (
            <>
              <p className="text-sm">
                {absence.delegate.displayName} remplace {user.displayName} du{' '}
                {formatDate(absence.startsAt)} au {formatDate(absence.endsAt)}.
                {/* Unnamed, it is the collaborator's own, or its author's account is gone. */}
                {absence.declaredBy && ` Absence déclarée par ${absence.declaredBy.displayName}.`}
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
                désigner ne lui en donne aucun. La personne voit qui a déclaré son absence et peut y
                mettre fin.{' '}
                {emailEnabled
                  ? 'Elle en est prévenue par e-mail.'
                  : 'Ce serveur n’envoie pas d’e-mail : prévenez-la.'}
              </p>
              <AbsenceForm
                candidates={candidates}
                submitLabel="Déclarer son absence"
                emptyNote="Aucun collègue, vous mis à part, ne détient le droit de valider des actions : personne ne peut remplacer ce collaborateur."
                declare={declare}
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
