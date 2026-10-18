'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Select } from '@/components/ui';
import { api, useAction } from '@/lib/api';
import { UNIT_TYPES } from '@/lib/labels';
import { unitOptions, type OrgUnitView, type OrganizationView } from './types';

/** Adds a sub-unit under `parent`, or edits `unit` (name, type, parent, lead). */
export function UnitForm({
  data,
  parent,
  unit,
  onDone,
}: {
  data: OrganizationView;
  parent?: OrgUnitView;
  unit?: OrgUnitView;
  onDone: (changed: boolean) => void;
}) {
  const [name, setName] = useState(unit?.name ?? '');
  const [type, setType] = useState(unit?.type ?? 'team');
  const [parentId, setParentId] = useState(unit?.parentId ?? parent?.id ?? '');
  const [leadUserId, setLeadUserId] = useState(unit?.leadUserId ?? '');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() =>
      unit
        ? api.patch(`/admin/org-units/${unit.id}`, {
            name,
            type,
            ...(unit.parentId && parentId !== unit.parentId ? { parentId } : {}),
            leadUserId: leadUserId || null,
          })
        : api.post('/admin/org-units', { parentId, name, type }),
    );
    if (saved) onDone(true);
  }

  return (
    <form onSubmit={submit} className="mt-2 flex flex-col gap-3 rounded-lg bg-surface-2 p-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Nom">
          <Input required value={name} onChange={(e) => setName(e.target.value)} maxLength={120} />
        </Field>
        <Field label="Type">
          <Select value={type} onChange={(e) => setType(e.target.value)}>
            {Object.entries(UNIT_TYPES)
              .filter(([key]) => key !== 'company')
              .map(([key, label]) => (
                <option key={key} value={key}>
                  {label}
                </option>
              ))}
          </Select>
        </Field>
        {unit?.parentId && (
          <Field label="Rattachée à">
            <Select value={parentId} onChange={(e) => setParentId(e.target.value)}>
              {unitOptions(data.units).map((o) => (
                <option key={o.id} value={o.id} disabled={o.id === unit.id}>
                  {o.label}
                </option>
              ))}
            </Select>
          </Field>
        )}
        {unit && (
          <Field label="Responsable (information, n’accorde aucun droit)">
            <Select value={leadUserId} onChange={(e) => setLeadUserId(e.target.value)}>
              <option value="">Aucun</option>
              {data.users.map((u) => (
                <option key={u.id} value={u.id}>
                  {u.displayName}
                </option>
              ))}
            </Select>
          </Field>
        )}
      </div>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {unit ? 'Enregistrer' : 'Ajouter'}
        </Button>
        <Button type="button" variant="ghost" onClick={() => onDone(false)}>
          Annuler
        </Button>
      </div>
    </form>
  );
}
