'use client';

import { Pencil, Trash2 } from 'lucide-react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type PermissionView, type RoleView } from '@/lib/api';
import { RoleForm } from './role-form';

/** One role and its permissions; custom roles can be edited or deleted. */
export function RoleCard({
  role,
  catalog,
  editing,
  onEdit,
  onChanged,
}: {
  role: RoleView;
  catalog: PermissionView[];
  editing: boolean;
  onEdit: (editing: boolean) => void;
  onChanged: () => void;
}) {
  const remove = useAction();
  const labels = new Map(catalog.map((p) => [p.permission, p]));
  // The server refuses these too; the buttons only explain why.
  const locked = role.isSystem || role.heldByYou;

  async function deleteRole() {
    if (!window.confirm(`Supprimer le rôle « ${role.name} » ?`)) return;
    if (await remove.run(() => api.delete(`/admin/roles/${role.id}`))) onChanged();
  }

  if (editing) {
    return (
      <Card className="p-4 md:col-span-2">
        <p className="mb-3 font-medium">Modifier « {role.name} »</p>
        <RoleForm
          catalog={catalog}
          role={role}
          onDone={(changed) => {
            onEdit(false);
            if (changed) onChanged();
          }}
        />
      </Card>
    );
  }

  return (
    <Card className="p-4">
      <div className="flex flex-wrap items-center gap-2">
        <p className="font-medium">{role.name}</p>
        {role.isSystem && <Badge>Rôle plateforme</Badge>}
        {role.heldByYou && <Badge tone="neutral">Vous le détenez</Badge>}
        <span className="text-xs text-muted">{role.assignments} habilitation(s)</span>
        {!locked && (
          <span className="ml-auto flex gap-1">
            <Button variant="ghost" className="px-2 py-1" onClick={() => onEdit(true)}>
              <Pencil className="size-3.5" aria-hidden />
              <span className="sr-only">Modifier {role.name}</span>
            </Button>
            <Button
              variant="ghost"
              className="px-2 py-1"
              onClick={deleteRole}
              loading={remove.pending}
              disabled={role.assignments > 0}
              title={role.assignments > 0 ? 'Retirez d’abord ses habilitations' : undefined}
            >
              <Trash2 className="size-3.5" aria-hidden />
              <span className="sr-only">Supprimer {role.name}</span>
            </Button>
          </span>
        )}
      </div>
      {role.description && <p className="mt-1 text-xs text-muted">{role.description}</p>}
      <ul className="mt-3 flex flex-wrap gap-1">
        {role.permissions.map((p) => (
          <li key={p}>
            <Badge tone={labels.get(p)?.delegable === false ? 'warn' : 'neutral'}>
              {labels.get(p)?.label ?? p}
            </Badge>
          </li>
        ))}
      </ul>
      {remove.error && <ErrorNote message={remove.error} />}
    </Card>
  );
}
