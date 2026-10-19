'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction, type PermissionView, type RoleView } from '@/lib/api';
import { groupCatalog } from './types';

/** Creates a custom role, or edits `role`: name, description, permissions. */
export function RoleForm({
  catalog,
  role,
  onDone,
}: {
  catalog: PermissionView[];
  role?: RoleView;
  onDone: (changed: boolean) => void;
}) {
  const [name, setName] = useState(role?.name ?? '');
  const [description, setDescription] = useState(role?.description ?? '');
  const [granted, setGranted] = useState(new Set(role?.permissions ?? []));
  const action = useAction();

  function toggle(permission: string) {
    setGranted((current) => {
      const next = new Set(current);
      if (next.has(permission)) next.delete(permission);
      else next.add(permission);
      return next;
    });
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = { name, description, permissions: [...granted] };
    const saved = await action.run(() =>
      role ? api.patch(`/admin/roles/${role.id}`, body) : api.post('/admin/roles', body),
    );
    if (saved) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-lg bg-surface-2 p-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Nom">
          <Input
            required
            minLength={2}
            maxLength={80}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        <Field label="Description">
          <Input
            maxLength={500}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </Field>
      </div>
      <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
        {groupCatalog(catalog).map(([group, permissions]) => (
          <fieldset key={group} className="flex flex-col gap-1 text-sm">
            <legend className="mb-1 text-xs font-medium text-muted">{group}</legend>
            {permissions.map((p) => (
              <label key={p.permission} className="flex items-start gap-2">
                <input
                  type="checkbox"
                  className="mt-1"
                  checked={granted.has(p.permission)}
                  onChange={() => toggle(p.permission)}
                />
                <span>
                  {p.label}
                  {!p.delegable && <span className="text-warn"> · jamais par l’agent</span>}
                </span>
              </label>
            ))}
          </fieldset>
        ))}
      </div>
      <p className="text-xs text-muted">
        Un rôle qui contient une permission d’administration (comptes, rôles, organisation, audit…)
        ne s’attribue que sur toute l’entreprise.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending} disabled={granted.size === 0}>
          {role ? 'Enregistrer' : 'Créer le rôle'}
        </Button>
        <Button type="button" variant="ghost" onClick={() => onDone(false)}>
          Annuler
        </Button>
      </div>
    </form>
  );
}
