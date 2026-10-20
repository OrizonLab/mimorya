'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction, type SkillUpdate, type SkillView } from '@/lib/api';

/** Changes the line the agent reads in its list of skills. */
export function DescriptionForm({
  skill,
  onDone,
}: {
  skill: SkillView;
  onDone: (changed: boolean) => void;
}) {
  const [description, setDescription] = useState(skill.description);
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = { description } satisfies SkillUpdate;
    if (await action.run(() => api.patch(`/admin/skills/${skill.id}`, body))) onDone(true);
  }

  return (
    <form onSubmit={submit} className="mt-2 flex flex-col gap-2 rounded-lg bg-surface-2 p-3">
      <Field label="Description (une ligne, lue par l’agent)">
        <Input
          required
          minLength={10}
          maxLength={300}
          value={description}
          onChange={(e) => setDescription(e.target.value)}
        />
      </Field>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          Enregistrer la description
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
