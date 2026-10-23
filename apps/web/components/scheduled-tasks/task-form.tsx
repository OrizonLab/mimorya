'use client';

import { useMemo, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Select, Textarea } from '@/components/ui';
import { useAction } from '@/lib/api';
import { useSyncedState } from '@/lib/use-synced-state';
import { timeZones } from './schedule';
import { WeekdayPicker } from './weekday-picker';

/** A task as its form writes it. */
export interface TaskDraft {
  title: string;
  instruction: string;
  weekdays: number[];
  time: string;
  timeZone: string;
}

/**
 * Writes a task: what it asks, on which days, at what time, in which time
 * zone. Taken again from `initial` when it changes (the list reloaded,
 * ADR-041); `clearOnSave` empties it once saved (a new task). The API
 * checks what is written; its refusal is shown here.
 */
export function TaskForm({
  initial,
  submitLabel,
  clearOnSave = false,
  onSave,
  onCancel,
}: {
  initial: TaskDraft;
  submitLabel: string;
  clearOnSave?: boolean;
  onSave: (draft: TaskDraft) => Promise<unknown>;
  onCancel?: () => void;
}) {
  const [draft, setDraft] = useSyncedState(initial);
  // From the draft: the one the reloaded list brings is among them.
  const zones = useMemo(() => timeZones(draft.timeZone), [draft.timeZone]);
  const action = useAction();
  const set = <K extends keyof TaskDraft>(key: K, value: TaskDraft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() => onSave(draft));
    if (saved !== null && clearOnSave) setDraft(initial);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <Field label="Intitulé">
        <Input
          required
          maxLength={120}
          value={draft.title}
          onChange={(e) => set('title', e.target.value)}
          placeholder="Ex. Point du lundi"
        />
      </Field>
      <Field label="Demande à l’agent">
        <Textarea
          required
          rows={3}
          maxLength={4000}
          value={draft.instruction}
          onChange={(e) => set('instruction', e.target.value)}
          placeholder="Ex. Fais le point sur les interventions de la semaine et signale les retards."
        />
      </Field>
      <WeekdayPicker value={draft.weekdays} onChange={(days) => set('weekdays', days)} />
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Heure">
          <Input
            type="time"
            required
            value={draft.time}
            onChange={(e) => set('time', e.target.value)}
          />
        </Field>
        <Field label="Fuseau horaire">
          <Select value={draft.timeZone} onChange={(e) => set('timeZone', e.target.value)}>
            {zones.map((zone) => (
              <option key={zone} value={zone}>
                {zone}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {submitLabel}
        </Button>
        {onCancel && (
          // Not while saving: what is sent may be saved all the same.
          <Button type="button" variant="ghost" disabled={action.pending} onClick={onCancel}>
            Annuler
          </Button>
        )}
      </div>
    </form>
  );
}
