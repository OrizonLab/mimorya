'use client';

import { ChevronDown, ChevronRight, Pencil, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { Badge, Button, Card, ErrorNote, LabelBadge } from '@/components/ui';
import { api, useAction, type SkillView } from '@/lib/api';
import { SKILL_STATUS } from '@/lib/labels';
import { DescriptionForm } from './description-form';
import { SkillVersions } from './skill-versions';

/** One skill: its state, and its versions once opened. */
export function SkillCard({
  skill,
  open,
  onOpen,
  onChanged,
}: {
  skill: SkillView;
  open: boolean;
  onOpen: (open: boolean) => void;
  onChanged: () => void;
}) {
  const [describing, setDescribing] = useState(false);
  const action = useAction();
  const active = skill.status === 'active';

  async function retire() {
    if (!window.confirm(`Retirer « ${skill.name} » ? L’agent ne la verra plus.`)) return;
    if (await action.run(() => api.delete(`/admin/skills/${skill.id}/activation`))) onChanged();
  }

  async function remove() {
    if (!window.confirm(`Supprimer « ${skill.name} » et toutes ses versions ?`)) return;
    if (await action.run(() => api.delete(`/admin/skills/${skill.id}`))) onChanged();
  }

  return (
    <Card className="p-4">
      <div className="flex flex-wrap items-center gap-2">
        <Button
          variant="ghost"
          className="px-1 py-0.5"
          aria-expanded={open}
          onClick={() => onOpen(!open)}
        >
          {open ? (
            <ChevronDown className="size-4" aria-hidden />
          ) : (
            <ChevronRight className="size-4" aria-hidden />
          )}
          <span className="font-mono text-sm">{skill.name}</span>
        </Button>
        <LabelBadge labels={SKILL_STATUS} value={skill.status} />
        {skill.activeVersion !== null && <Badge tone="ok">v{skill.activeVersion} active</Badge>}
        {skill.latestVersion !== null && skill.latestVersion !== skill.activeVersion && (
          <Badge>dernière : v{skill.latestVersion}</Badge>
        )}
        <span className="ml-auto flex gap-1">
          <Button variant="ghost" className="px-2 py-1" onClick={() => setDescribing(true)}>
            <Pencil className="size-3.5" aria-hidden />
            <span className="sr-only">Modifier la description de {skill.name}</span>
          </Button>
          {active ? (
            <Button variant="ghost" className="px-2 py-1" onClick={retire} loading={action.pending}>
              Retirer
            </Button>
          ) : (
            <Button variant="ghost" className="px-2 py-1" onClick={remove} loading={action.pending}>
              <Trash2 className="size-3.5" aria-hidden />
              <span className="sr-only">Supprimer {skill.name}</span>
            </Button>
          )}
        </span>
      </div>
      {describing ? (
        <DescriptionForm
          skill={skill}
          onDone={(changed) => {
            setDescribing(false);
            if (changed) onChanged();
          }}
        />
      ) : (
        <p className="mt-1 text-sm text-muted">{skill.description}</p>
      )}
      {action.error && <ErrorNote message={action.error} />}
      {open && <SkillVersions skill={skill} onChanged={onChanged} />}
    </Card>
  );
}
