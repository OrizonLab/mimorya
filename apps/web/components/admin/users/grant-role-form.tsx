'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Select } from '@/components/ui';
import { api, useAction, type OrgUnitView, type RoleView } from '@/lib/api';
import { unitOptions } from '../organization/types';

type ScopeChoice = 'tenant' | 'subtree' | 'unit' | 'self';

const SCOPES: Record<ScopeChoice, string> = {
  tenant: 'Toute l’entreprise',
  subtree: 'Une unité et ses sous-unités',
  unit: 'Une unité seule',
  self: 'Ses propres données',
};

function scopeBody(choice: ScopeChoice, orgUnitId: string) {
  if (choice === 'tenant' || choice === 'self') return { type: choice };
  return { type: 'org_unit', orgUnitId, includeDescendants: choice === 'subtree' };
}

/** Grants a role on a scope, optionally until a date (`role:manage`). */
export function GrantRoleForm({
  userId,
  roles,
  units,
  onGranted,
}: {
  userId: string;
  roles: RoleView[];
  units: OrgUnitView[];
  onGranted: () => void;
}) {
  const [roleId, setRoleId] = useState(roles[0]?.id ?? '');
  const [scope, setScope] = useState<ScopeChoice>('subtree');
  const [orgUnitId, setOrgUnitId] = useState(units[0]?.id ?? '');
  const [until, setUntil] = useState('');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const granted = await action.run(() =>
      api.post(`/admin/users/${userId}/assignments`, {
        roleId,
        scope: scopeBody(scope, orgUnitId),
        expiresAt: until ? new Date(`${until}T23:59:59`).toISOString() : null,
      }),
    );
    if (granted) onGranted();
  }

  const needsUnit = scope === 'subtree' || scope === 'unit';
  return (
    <form onSubmit={submit} className="flex flex-col gap-3 rounded-lg bg-surface-2 p-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Rôle">
          <Select value={roleId} onChange={(e) => setRoleId(e.target.value)}>
            {roles.map((r) => (
              <option key={r.id} value={r.id}>
                {r.name}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Périmètre">
          <Select value={scope} onChange={(e) => setScope(e.target.value as ScopeChoice)}>
            {Object.entries(SCOPES).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </Select>
        </Field>
        {needsUnit && (
          <Field label="Unité">
            <Select value={orgUnitId} onChange={(e) => setOrgUnitId(e.target.value)}>
              {unitOptions(units).map((o) => (
                <option key={o.id} value={o.id}>
                  {o.label}
                </option>
              ))}
            </Select>
          </Field>
        )}
        <Field label="Jusqu’au (facultatif)">
          <Input type="date" value={until} onChange={(e) => setUntil(e.target.value)} />
        </Field>
      </div>
      <p className="text-xs text-muted">
        Les rôles d’administration ne s’attribuent que sur toute l’entreprise.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <Button type="submit" className="w-fit" loading={action.pending}>
        Attribuer le rôle
      </Button>
    </form>
  );
}
