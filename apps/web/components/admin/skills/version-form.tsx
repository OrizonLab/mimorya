'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote } from '@/components/ui';
import {
  api,
  useAction,
  type NewSkillVersion,
  type SkillVersionCreated,
  type SkillVersionView,
} from '@/lib/api';
import { SkillContentFields } from './skill-form';

/**
 * Writes a new version, a draft: the active one stays until another is made
 * active. It starts from `from`, its content and its quality criteria
 * (ADR-106), empty without one.
 */
export function VersionForm({
  skillId,
  from,
  onDone,
}: {
  skillId: string;
  from: SkillVersionView | undefined;
  onDone: (saved: boolean) => void;
}) {
  const initialContent = from?.content ?? '';
  const initialCriteria = from?.qualityCriteria.join('\n') ?? '';
  const [content, setContent] = useState(initialContent);
  const [criteria, setCriteria] = useState(initialCriteria);
  const [changelog, setChangelog] = useState('');
  const action = useAction();
  const unchanged = content === initialContent && criteria === initialCriteria;

  async function submit(event: FormEvent) {
    event.preventDefault();
    // Sent as they stand, none included: a version keeps the criteria it is written with.
    const body = {
      content,
      qualityCriteria: criteria.split('\n'),
      changelog,
    } satisfies NewSkillVersion;
    const created = await action.run(() =>
      api.post<SkillVersionCreated>(`/admin/skills/${skillId}/versions`, body),
    );
    if (created) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-lg bg-surface-2 p-3">
      <SkillContentFields
        content={content}
        criteria={criteria}
        changelog={changelog}
        onContent={setContent}
        onCriteria={setCriteria}
        onChangelog={setChangelog}
      />
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending} disabled={unchanged}>
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
