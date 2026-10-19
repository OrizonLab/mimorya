'use client';

import { Pencil, Plus, Trash2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { Badge, Button, ErrorNote } from '@/components/ui';
import { api, useAction, type OrgUnitView, type Organization } from '@/lib/api';
import { UNIT_TYPES } from '@/lib/labels';
import { UnitForm } from './unit-form';

export type Editing = { unitId: string; mode: 'add' | 'edit' } | null;

/** One unit of the tree, its members, and — for managers — its actions. */
export function UnitNode({
  data,
  unit,
  depth,
  editing,
  onEdit,
  onChanged,
  children,
}: {
  data: Organization;
  unit: OrgUnitView;
  depth: number;
  editing: Editing;
  onEdit: (editing: Editing) => void;
  onChanged: () => void;
  /** The sub-units. */
  children: ReactNode;
}) {
  const remove = useAction();
  const name = (userId: string) => data.users.find((u) => u.id === userId)?.displayName;
  const mode = editing?.unitId === unit.id ? editing.mode : null;
  const done = (changed: boolean) => {
    onEdit(null);
    if (changed) onChanged();
  };

  async function deleteUnit() {
    if (!window.confirm(`Supprimer l’unité « ${unit.name} » ?`)) return;
    if (await remove.run(() => api.delete(`/admin/org-units/${unit.id}`))) onChanged();
  }

  return (
    <li style={{ marginLeft: depth * 16 }} className="py-1.5">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">{unit.name}</span>
        <Badge>{UNIT_TYPES[unit.type] ?? unit.type}</Badge>
        {unit.leadUserId && (
          <span className="text-xs text-muted">Responsable : {name(unit.leadUserId)}</span>
        )}
        {data.canManage && (
          <span className="flex gap-1">
            <Button
              variant="ghost"
              className="px-2 py-1"
              onClick={() => onEdit({ unitId: unit.id, mode: 'add' })}
            >
              <Plus className="size-3.5" aria-hidden />
              <span className="sr-only">Ajouter une sous-unité à {unit.name}</span>
            </Button>
            <Button
              variant="ghost"
              className="px-2 py-1"
              onClick={() => onEdit({ unitId: unit.id, mode: 'edit' })}
            >
              <Pencil className="size-3.5" aria-hidden />
              <span className="sr-only">Modifier {unit.name}</span>
            </Button>
            {unit.parentId && (
              <Button
                variant="ghost"
                className="px-2 py-1"
                onClick={deleteUnit}
                loading={remove.pending}
              >
                <Trash2 className="size-3.5" aria-hidden />
                <span className="sr-only">Supprimer {unit.name}</span>
              </Button>
            )}
          </span>
        )}
      </div>
      {unit.members.length > 0 && (
        <p className="mt-0.5 text-xs text-muted">{unit.members.map(name).join(' · ')}</p>
      )}
      {remove.error && <ErrorNote message={remove.error} />}
      {mode === 'add' && <UnitForm data={data} parent={unit} onDone={done} />}
      {mode === 'edit' && <UnitForm data={data} unit={unit} onDone={done} />}
      <ul>{children}</ul>
    </li>
  );
}
