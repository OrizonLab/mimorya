'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Textarea } from '@/components/ui';
import { api, useAction, type Created, type NewMission } from '@/lib/api';

/** A mission as its form writes it: its criteria one per line. */
interface MissionDraft {
  title: string;
  objective: string;
  criteria: string;
}

const BLANK: MissionDraft = { title: '', objective: '', criteria: '' };

/**
 * Entrusts a mission to the agent (ADR-089): a title, what it is to
 * achieve, and the criteria its final answer is reviewed against, one per
 * line. The API checks what is written; its refusal (too many missions
 * under way, criteria out of bounds) is shown here. Emptied once entrusted.
 */
export function MissionForm({ onEntrusted }: { onEntrusted: () => void }) {
  const [draft, setDraft] = useState<MissionDraft>(BLANK);
  const action = useAction();
  const set = <K extends keyof MissionDraft>(key: K, value: MissionDraft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  async function submit(event: FormEvent) {
    event.preventDefault();
    const entrusted = await action.run(() =>
      api.post<Created>('/missions', {
        title: draft.title,
        objective: draft.objective,
        successCriteria: draft.criteria
          .split('\n')
          .map((line) => line.trim())
          .filter((line) => line !== ''),
      } satisfies NewMission),
    );
    if (entrusted === null) return;
    setDraft(BLANK);
    onEntrusted();
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <Field label="Intitulé">
        <Input
          required
          maxLength={120}
          value={draft.title}
          onChange={(e) => set('title', e.target.value)}
          placeholder="Ex. Suivi des interventions de la semaine"
        />
      </Field>
      <Field label="Objectif">
        <Textarea
          required
          rows={4}
          maxLength={4000}
          value={draft.objective}
          onChange={(e) => set('objective', e.target.value)}
          placeholder="Ex. Relis les comptes rendus de la semaine, repère les interventions non terminées et propose leur suivi."
        />
      </Field>
      <Field label="Critères de réussite (un par ligne)">
        <Textarea
          required
          rows={3}
          value={draft.criteria}
          onChange={(e) => set('criteria', e.target.value)}
          placeholder="Ex. Chaque intervention non terminée est nommée"
        />
      </Field>
      {action.error && <ErrorNote message={action.error} />}
      <div>
        <Button type="submit" loading={action.pending}>
          Confier la mission
        </Button>
      </div>
    </form>
  );
}
