'use client';

import { useState, type FormEvent } from 'react';
import { Button, Card, Checkbox, ErrorNote, Loaded } from '@/components/ui';
import { api, useAction, useApiData, type NotificationSettings } from '@/lib/api';
import { useSyncedState } from '@/lib/use-synced-state';

const NOTIFICATIONS = '/account/notifications';

/** Each kind of e-mail one may choose not to receive, with what it is. */
const KINDS: { key: keyof NotificationSettings; label: string; detail: string }[] = [
  {
    key: 'approvals',
    label: 'Les demandes qui attendent ma décision',
    detail:
      'Les validations qui m’attendent comme responsable, ou à la place d’un responsable absent, avec la relance la veille de leur échéance ; les compétences que l’agent propose, si je les administre. Sans cet e-mail, elles vous attendent dans « Validations » ou dans l’onglet « Compétences » ; les validations expirent à leur échéance.',
  },
  {
    key: 'ownRuns',
    label: 'Mes tâches planifiées et mes missions qui attendent ma validation',
    detail:
      'Leur exécution reste en pause jusqu’à votre décision, que vous trouvez dans leur conversation.',
  },
  {
    key: 'missionEnds',
    label: 'La fin de mes missions',
    detail: 'Menées à bien ou arrêtées inachevées, sauf celles que vous arrêtez vous-même.',
  },
];

/** The e-mails the signed-in person receives from the platform (ADR-094). */
export function NotificationForm() {
  const state = useApiData<NotificationSettings>(NOTIFICATIONS);
  return (
    <Loaded state={state}>
      {(data) => <NotificationChoices data={data} onSaved={state.reload} />}
    </Loaded>
  );
}

function NotificationChoices({
  data,
  onSaved,
}: {
  data: NotificationSettings;
  onSaved: () => void;
}) {
  // Started again from each reloaded choice (ADR-041).
  const [choice, setChoice] = useSyncedState(data);
  const [done, setDone] = useState(false);
  const action = useAction();
  const unchanged = KINDS.every(({ key }) => choice[key] === data[key]);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setDone(false);
    const body = {
      approvals: choice.approvals,
      ownRuns: choice.ownRuns,
      missionEnds: choice.missionEnds,
    } satisfies NotificationSettings;
    if (await action.run(() => api.put(NOTIFICATIONS, body))) {
      setDone(true);
      onSaved();
    }
  }

  return (
    <Card className="max-w-2xl p-4">
      <form aria-label="E-mails reçus" onSubmit={submit} className="flex flex-col gap-3">
        <p className="font-medium">E-mails reçus</p>
        {KINDS.map(({ key, label, detail }) => (
          <div key={key} className="flex flex-col gap-0.5">
            <label className="flex items-center gap-2 text-sm">
              <Checkbox
                checked={choice[key]}
                onChange={(e) => {
                  setDone(false);
                  setChoice({ ...choice, [key]: e.target.checked });
                }}
              />
              {label}
            </label>
            <p className="ml-6 text-xs text-muted">{detail}</p>
          </div>
        ))}
        <p className="text-xs text-muted">
          Les e-mails d’invitation, de réinitialisation du mot de passe et d’absence déclarée pour
          vous par un administrateur sont toujours envoyés.
        </p>
        {action.error && <ErrorNote message={action.error} />}
        {done && (
          <p role="status" className="text-sm text-ok">
            Choix enregistré.
          </p>
        )}
        <Button type="submit" className="w-fit" loading={action.pending} disabled={unchanged}>
          Enregistrer
        </Button>
      </form>
    </Card>
  );
}
