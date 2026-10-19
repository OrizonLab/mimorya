'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Select } from '@/components/ui';
import { api, useAction, type DirectoryUser, type OrgUnitView, type Sensitivity } from '@/lib/api';
import { CLEARANCE_LABELS } from '@/lib/labels';
import { unitOptions } from '../organization/types';
import { CredentialActions } from './credential-actions';
import type { AccountNotice } from './types';

/** Profile, status and password (or invitation) of an account (`user:manage`). */
export function ProfileEditor({
  user,
  units,
  emailEnabled,
  onChanged,
  onNotice,
}: {
  user: DirectoryUser;
  units: OrgUnitView[];
  emailEnabled: boolean;
  onChanged: () => void;
  onNotice: (notice: AccountNotice) => void;
}) {
  const [displayName, setDisplayName] = useState(user.displayName);
  const [jobTitle, setJobTitle] = useState(user.jobTitle ?? '');
  const [orgUnitId, setOrgUnitId] = useState(user.orgUnitId ?? '');
  const [clearance, setClearance] = useState(user.clearance ?? 'internal');
  const action = useAction();
  // An invited account is enabled: it is waiting for its password.
  const active = user.status !== 'disabled';

  async function save(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() =>
      api.patch(`/admin/users/${user.id}`, {
        displayName,
        jobTitle: jobTitle || null,
        orgUnitId: orgUnitId || null,
        clearance,
      }),
    );
    if (saved) onChanged();
  }

  async function toggleStatus() {
    const message = active
      ? `Désactiver le compte de ${user.displayName} ? Ses sessions seront fermées.`
      : `Réactiver le compte de ${user.displayName} ?`;
    if (!window.confirm(message)) return;
    const status = active ? 'disabled' : 'active';
    if (await action.run(() => api.patch(`/admin/users/${user.id}`, { status }))) onChanged();
  }

  return (
    <form onSubmit={save} className="flex flex-col gap-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Nom affiché">
          <Input
            required
            minLength={2}
            maxLength={120}
            value={displayName}
            onChange={(e) => setDisplayName(e.target.value)}
          />
        </Field>
        <Field label="Fonction">
          <Input maxLength={120} value={jobTitle} onChange={(e) => setJobTitle(e.target.value)} />
        </Field>
        <Field label="Unité de rattachement">
          <Select value={orgUnitId} onChange={(e) => setOrgUnitId(e.target.value)}>
            <option value="">Aucune</option>
            {unitOptions(units).map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Niveau d’habilitation aux données">
          <Select value={clearance} onChange={(e) => setClearance(e.target.value as Sensitivity)}>
            {Object.entries(CLEARANCE_LABELS).map(([key, label]) => (
              <option key={key} value={key}>
                {label}
              </option>
            ))}
          </Select>
        </Field>
      </div>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex flex-wrap gap-2">
        <Button type="submit" loading={action.pending}>
          Enregistrer
        </Button>
        <CredentialActions
          user={user}
          emailEnabled={emailEnabled}
          action={action}
          onNotice={onNotice}
        />
        <Button type="button" variant={active ? 'danger' : 'secondary'} onClick={toggleStatus}>
          {active ? 'Désactiver le compte' : 'Réactiver le compte'}
        </Button>
      </div>
    </form>
  );
}
