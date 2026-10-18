'use client';

import { useState, type FormEvent } from 'react';
import { Button, Card, ErrorNote, Field, Input, Select } from '@/components/ui';
import { api, useAction } from '@/lib/api';
import { CLEARANCE_LABELS } from '@/lib/labels';
import { unitOptions, type OrgUnitView } from '../organization/types';

/** Creates an account; the parent shows the temporary password it receives. */
export function NewUserForm({
  units,
  onCreated,
}: {
  units: OrgUnitView[];
  onCreated: (displayName: string, temporaryPassword: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [email, setEmail] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [jobTitle, setJobTitle] = useState('');
  const [orgUnitId, setOrgUnitId] = useState('');
  const [clearance, setClearance] = useState('internal');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const created = await action.run(() =>
      api.post<{ temporaryPassword: string }>('/admin/users', {
        email,
        displayName,
        jobTitle,
        orgUnitId: orgUnitId || null,
        clearance,
      }),
    );
    if (!created) return;
    onCreated(displayName, created.temporaryPassword);
    setEmail('');
    setDisplayName('');
    setJobTitle('');
    setOpen(false);
  }

  if (!open) {
    return (
      <Button className="mb-4" onClick={() => setOpen(true)}>
        Ajouter un collaborateur
      </Button>
    );
  }
  return (
    <Card className="mb-4 p-4">
      <form onSubmit={submit} className="flex flex-col gap-3">
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
          <Field label="Adresse e-mail">
            <Input required type="email" value={email} onChange={(e) => setEmail(e.target.value)} />
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
            <Select value={clearance} onChange={(e) => setClearance(e.target.value)}>
              {Object.entries(CLEARANCE_LABELS).map(([key, label]) => (
                <option key={key} value={key}>
                  {label}
                </option>
              ))}
            </Select>
          </Field>
        </div>
        <p className="text-xs text-muted">
          Le compte reçoit le rôle « Collaborateur » sur ses propres données ; attribuez ensuite les
          rôles métier depuis sa fiche.
        </p>
        {action.error && <ErrorNote message={action.error} />}
        <div className="flex gap-2">
          <Button type="submit" loading={action.pending}>
            Créer le compte
          </Button>
          <Button type="button" variant="ghost" onClick={() => setOpen(false)}>
            Annuler
          </Button>
        </div>
      </form>
    </Card>
  );
}
