'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote } from '@/components/ui';
import { api, useAction, type NewSkillVersion, type SkillVersionCreated } from '@/lib/api';
import { SkillContentFields } from './skill-form';

/** Writes a new version, a draft: the active one stays until another is made active. */
export function VersionForm({
  skillId,
  initial,
  onDone,
}: {
  skillId: string;
  initial: string;
  onDone: (saved: boolean) => void;
}) {
  const [content, setContent] = useState(initial);
  const [changelog, setChangelog] = useState('');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = { content, changelog } satisfies NewSkillVersion;
    const created = await action.run(() =>
      api.post<SkillVersionCreated>(`/admin/skills/${skillId}/versions`, body),
    );
    if (created) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-lg bg-surface-2 p-3">
      <SkillContentFields
        content={content}
        changelog={changelog}
        onContent={setContent}
        onChangelog={setChangelog}
      />
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending} disabled={content === initial}>
          Enregistrer la version
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
