'use client';

import { Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type ApprovalDelegations } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { useCan } from '@/lib/me-context';
import { AbsenceForm } from './absence-form';

/**
 * One's absence and the absences one covers (ADR-071, ADR-073). Whoever
 * validates others' actions names a colleague who validates in their stead
 * meanwhile, where that colleague holds the right to: the naming grants
 * none. One an account administrator declared says so (ADR-078). Shown to
 * those who validate, or stand in for someone.
 */
export function AbsenceSection({
  data,
  onChanged,
}: {
  data: ApprovalDelegations;
  onChanged: () => void;
}) {
  const validates = useCan('approval:decide');
  const action = useAction();
  if (!validates && !data.own && data.standingInFor.length === 0) return null;

  async function end() {
    if (await action.run(() => api.delete('/approval-delegation'))) onChanged();
  }

  return (
    <Card className="mb-6 flex flex-col gap-3 p-4">
      <p className="font-medium">Absence</p>
      {data.standingInFor.map((d) => (
        <p key={d.absent.id} className="text-sm">
          Vous remplacez {d.absent.displayName} du {formatDate(d.startsAt)} au{' '}
          {formatDate(d.endsAt)} : ses demandes que vous avez le droit de valider s’affichent
          ci-dessous.
        </p>
      ))}
      {data.own ? (
        <>
          <p className="text-sm">
            {data.own.delegate.displayName} vous remplace du {formatDate(data.own.startsAt)} au{' '}
            {formatDate(data.own.endsAt)}. Les demandes qui vous attendent lui sont présentées, avec
            leurs e-mails, quand ce collègue détient le droit de les valider ; vous pouvez toujours
            les décider vous-même.
          </p>
          {data.own.declaredBy && (
            <p className="text-sm">
              Absence déclarée pour vous par {data.own.declaredBy.displayName}, depuis
              l’administration.
            </p>
          )}
          {action.error && <ErrorNote message={action.error} />}
          <Button variant="danger" className="w-fit" loading={action.pending} onClick={end}>
            Mettre fin à l’absence
          </Button>
        </>
      ) : (
        validates && (
          <>
            <p className="text-sm text-muted">
              Pendant une absence, le collègue que vous désignez valide à votre place les demandes
              qui vous attendent, s’il détient le droit de valider les actions des personnes
              concernées : le désigner ne lui en donne aucun.
            </p>
            <AbsenceForm
              candidates={data.candidates}
              submitLabel="Déclarer mon absence"
              emptyNote="Aucun collègue ne détient le droit de valider des actions : personne ne peut vous remplacer."
              declare={(body) => api.put('/approval-delegation', body)}
              onDeclared={onChanged}
            />
          </>
        )
      )}
    </Card>
  );
}
