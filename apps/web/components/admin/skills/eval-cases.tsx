'use client';

import { Pencil, Plus, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { Button, ErrorNote } from '@/components/ui';
import { api, useAction, type SkillEvalCaseView } from '@/lib/api';
import { CaseForm } from './case-form';

/** Cases a skill holds at most (`skill_eval_cases::MAX_CASES_PER_SKILL`). */
const MAX_CASES = 10;

/**
 * The cases a skill's versions are tried on (ADR-103): written, changed
 * and deleted here; an evaluation plays them as they stand when it is
 * asked for.
 */
export function EvalCases({
  skillId,
  cases,
  onChanged,
}: {
  skillId: string;
  cases: SkillEvalCaseView[];
  onChanged: () => void;
}) {
  // The case written: `new`, one's id, or none.
  const [editing, setEditing] = useState<string | null>(null);
  const action = useAction();

  function done(saved: boolean) {
    setEditing(null);
    if (saved) onChanged();
  }

  async function remove(item: SkillEvalCaseView) {
    if (!window.confirm(`Supprimer le cas « ${item.name} » ?`)) return;
    if (await action.run(() => api.delete(`/admin/skill-cases/${item.id}`))) onChanged();
  }

  return (
    <section className="flex flex-col gap-2" aria-label="Cas d’évaluation">
      <h3 className="text-sm font-medium">Cas d’évaluation</h3>
      {cases.length === 0 && editing !== 'new' && (
        <p className="text-xs text-muted">
          Aucun cas : écrivez une demande type et ce qu’une bonne réponse doit faire, pour essayer
          une version avant de l’activer.
        </p>
      )}
      <ul className="flex flex-col gap-2">
        {cases.map((item) =>
          editing === item.id ? (
            <li key={item.id}>
              <CaseForm skillId={skillId} initial={item} onDone={done} />
            </li>
          ) : (
            <li key={item.id} className="rounded-lg bg-surface-2 p-3 text-sm">
              <div className="flex items-center gap-2">
                <span className="font-medium">{item.name}</span>
                <span className="ml-auto flex gap-1">
                  <Button variant="ghost" className="px-2 py-1" onClick={() => setEditing(item.id)}>
                    <Pencil className="size-3.5" aria-hidden />
                    <span className="sr-only">Modifier le cas {item.name}</span>
                  </Button>
                  <Button
                    variant="ghost"
                    className="px-2 py-1"
                    loading={action.pending}
                    onClick={() => remove(item)}
                  >
                    <Trash2 className="size-3.5" aria-hidden />
                    <span className="sr-only">Supprimer le cas {item.name}</span>
                  </Button>
                </span>
              </div>
              <p className="mt-1 whitespace-pre-wrap">{item.request}</p>
              <ul className="mt-1 list-disc pl-5 text-xs text-muted">
                {item.criteria.map((criterion, index) => (
                  <li key={`${index} ${criterion}`}>{criterion}</li>
                ))}
              </ul>
            </li>
          ),
        )}
      </ul>
      {action.error && <ErrorNote message={action.error} />}
      {editing === 'new' ? (
        <CaseForm skillId={skillId} onDone={done} />
      ) : (
        cases.length < MAX_CASES && (
          <div>
            <Button variant="secondary" onClick={() => setEditing('new')}>
              <Plus className="size-4" aria-hidden /> Nouveau cas
            </Button>
          </div>
        )
      )}
    </section>
  );
}
