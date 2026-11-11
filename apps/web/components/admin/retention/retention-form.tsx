'use client';

import type { FormEvent } from 'react';
import { Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type RetentionUpdate, type RetentionView } from '@/lib/api';
import { useSyncedState } from '@/lib/use-synced-state';
import { DurationField, inUnit } from './duration-field';

const RETENTION = '/admin/retention';

type Draft = Record<RetentionView['durations'][number]['kind'], string>;

/** The durations as the form writes them. */
function draftOf(data: RetentionView): Draft {
  const draft: Draft = { conversations: '', episodes: '', departed: '' };
  for (const duration of data.durations) draft[duration.kind] = String(duration.duration);
  return draft;
}

/**
 * The durations changed to shorter than what the purge applies now, said:
 * one asked as it already is changes nothing, a shortening that waits
 * included, as the API leaves it.
 */
function shortenings(data: RetentionView, draft: Draft): string[] {
  return data.durations
    .filter((duration) => {
      const asked = Number(draft[duration.kind]);
      return asked !== duration.duration && asked < duration.inEffect;
    })
    .map((duration) => `${duration.label} : ${inUnit(duration, Number(draft[duration.kind]))}`);
}

export function RetentionForm({ data, onSaved }: { data: RetentionView; onSaved: () => void }) {
  // Started again from each reloaded setting (ADR-041).
  const [draft, setDraft] = useSyncedState(draftOf(data));
  const action = useAction();
  // `06` is the `6` saved: nothing to save.
  const unchanged = data.durations.every(
    (duration) => Number(draft[duration.kind]) === duration.duration,
  );

  async function submit(event: FormEvent) {
    event.preventDefault();
    const shorter = shortenings(data, draft);
    if (
      shorter.length > 0 &&
      !window.confirm(
        `Raccourcir ${shorter.join(', ')} ? Dans ${data.shorteningDelayDays} jours, la purge ` +
          'supprimera ce qui n’est plus gardé, sans retour possible.',
      )
    ) {
      return;
    }
    const body = {
      conversationMonths: Number(draft.conversations),
      episodeMonths: Number(draft.episodes),
      departedDays: Number(draft.departed),
    } satisfies RetentionUpdate;
    if (await action.run(() => api.put(RETENTION, body))) onSaved();
  }

  return (
    <Card className="p-4">
      <form aria-label="Durées de conservation" onSubmit={submit} className="flex flex-col gap-3">
        <p className="font-medium">Durées de conservation</p>
        <div className="grid gap-4 sm:grid-cols-3">
          {data.durations.map((duration) => (
            <DurationField
              key={duration.kind}
              duration={duration}
              value={draft[duration.kind]}
              onChange={(value) => setDraft({ ...draft, [duration.kind]: value })}
            />
          ))}
        </div>
        <p className="text-xs text-muted">
          Une purge passe chaque heure. Un allongement s’applique aussitôt ; un raccourcissement{' '}
          {data.shorteningDelayDays} jours plus tard, pour laisser le temps de revenir en arrière.
          Chaque changement est tracé dans le journal d’audit.
        </p>
        {action.error && <ErrorNote message={action.error} />}
        <div>
          <Button type="submit" loading={action.pending} disabled={unchanged}>
            Enregistrer
          </Button>
        </div>
      </form>
    </Card>
  );
}
