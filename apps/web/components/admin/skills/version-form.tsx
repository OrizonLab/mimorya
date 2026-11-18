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
 * active. It starts from `from`, its content, its quality criteria
 * (ADR-106) and whether it asks for the escalation model (ADR-207), empty
 * without one.
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
  const initialEscalation = from?.modelEscalation ?? false;
  const [escalation, setEscalation] = useState(initialEscalation);
  const [changelog, setChangelog] = useState('');
  const action = useAction();
  const unchanged =
    content === initialContent &&
    kept(criteria) === kept(initialCriteria) &&
    escalation === initialEscalation;

  async function submit(event: FormEvent) {
    event.preventDefault();
    // Sent as they stand, none included: a version keeps the criteria it is
    // written with, and asks for the escalation model or not, as shown.
    const body = {
      content,
      qualityCriteria: criteria.split('\n'),
      modelEscalation: escalation,
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
        escalation={escalation}
        changelog={changelog}
        onContent={setContent}
        onCriteria={setCriteria}
        onEscalation={setEscalation}
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

/** Criteria as the server keeps them: each trimmed, the blank ones left out. */
function kept(criteria: string): string {
  return criteria
    .split('\n')
    .map((line) => line.trim())
    .filter((line) => line !== '')
    .join('\n');
}
