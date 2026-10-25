'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Textarea } from '@/components/ui';
import { api, useAction, type SkillEvalCaseInput, type SkillEvalCaseView } from '@/lib/api';

/**
 * Writes an evaluation case of a skill (ADR-103), or changes `initial`: a
 * request as a person would ask it, and what a good answer does, one
 * criterion per line.
 */
export function CaseForm({
  skillId,
  initial,
  onDone,
}: {
  skillId: string;
  initial?: SkillEvalCaseView;
  onDone: (saved: boolean) => void;
}) {
  const [name, setName] = useState(initial?.name ?? '');
  const [request, setRequest] = useState(initial?.request ?? '');
  const [criteria, setCriteria] = useState(initial?.criteria.join('\n') ?? '');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = { name, request, criteria: criteria.split('\n') } satisfies SkillEvalCaseInput;
    const saved = await action.run(() =>
      initial
        ? api.put(`/admin/skill-cases/${initial.id}`, body)
        : api.post(`/admin/skills/${skillId}/cases`, body),
    );
    if (saved) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-lg bg-surface-2 p-3">
      <Field label="Nom du cas">
        <Input
          required
          minLength={2}
          maxLength={80}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </Field>
      <Field label="Demande (posée comme le ferait un collaborateur)">
        <Textarea
          required
          rows={3}
          maxLength={4000}
          value={request}
          onChange={(e) => setRequest(e.target.value)}
        />
      </Field>
      <Field label="Critères d’une bonne réponse (un par ligne, 8 au plus)">
        <Textarea
          required
          rows={3}
          value={criteria}
          onChange={(e) => setCriteria(e.target.value)}
        />
      </Field>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {initial ? 'Enregistrer le cas' : 'Ajouter le cas'}
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
