'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Button, Card, EmptyState, Loaded } from '@/components/ui';
import { useApiData, type SkillList } from '@/lib/api';
import { SkillCard } from './skill-card';
import { SkillForm } from './skill-form';

/** The company's skills: written here, made active, retired (ADR-052). */
export function SkillsTab() {
  const state = useApiData<SkillList>('/admin/skills');
  const [creating, setCreating] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-4">
          {creating ? (
            <Card className="p-4">
              <p className="mb-3 font-medium">Nouvelle compétence</p>
              <SkillForm
                onDone={(changed) => {
                  setCreating(false);
                  if (changed) state.reload();
                }}
              />
            </Card>
          ) : (
            <div>
              <Button variant="secondary" onClick={() => setCreating(true)}>
                <Plus className="size-4" aria-hidden /> Nouvelle compétence
              </Button>
            </div>
          )}
          {data.skills.length === 0 ? (
            <EmptyState title="Aucune compétence" />
          ) : (
            data.skills.map((skill) => (
              <SkillCard
                key={skill.id}
                skill={skill}
                open={open === skill.id}
                onOpen={(on) => setOpen(on ? skill.id : null)}
                onChanged={state.reload}
              />
            ))
          )}
          <p className="text-xs text-muted">
            Une compétence est une procédure que l’agent charge avant une tâche qu’elle couvre. Elle
            n’atteint aucun échange tant qu’aucune de ses versions n’est active ; une version écrite
            ne change plus, et une version plus ancienne peut redevenir active. Une compétence ne
            peut ni étendre les droits de l’agent ni lui éviter une validation.
          </p>
        </div>
      )}
    </Loaded>
  );
}
