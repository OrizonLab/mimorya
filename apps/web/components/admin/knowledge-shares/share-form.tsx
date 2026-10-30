'use client';

import { useState, type FormEvent } from 'react';
import { Button, Checkbox, ErrorNote, Field, Input, Select } from '@/components/ui';
import {
  api,
  useAction,
  type NewKnowledgeShare,
  type Sensitivity,
  type ShareableUnit,
} from '@/lib/api';
import { CLEARANCE_LABELS } from '@/lib/labels';
import { unitOptions } from '../organization/types';

const SENSITIVITIES: Sensitivity[] = ['public', 'internal', 'confidential', 'restricted'];

/** Shares a unit's knowledge with another unit, up to a sensitivity and a date (ADR-143). */
export function ShareForm({
  units,
  onDone,
}: {
  units: ShareableUnit[];
  onDone: (changed: boolean) => void;
}) {
  const options = unitOptions(units);
  const [source, setSource] = useState(options[0]?.id ?? '');
  const [target, setTarget] = useState(options[1]?.id ?? '');
  const [below, setBelow] = useState(false);
  const [memory, setMemory] = useState(true);
  const [documents, setDocuments] = useState(true);
  const [ceiling, setCeiling] = useState<Sensitivity>('internal');
  const [until, setUntil] = useState('');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const shared = await action.run(() =>
      api.post('/admin/knowledge-shares', {
        sourceOrgUnitId: source,
        targetOrgUnitId: target,
        includeDescendants: below,
        memory,
        documents,
        maxSensitivity: ceiling,
        expiresAt: until ? new Date(`${until}T23:59:59`).toISOString() : undefined,
      } satisfies NewKnowledgeShare),
    );
    if (shared) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-4 rounded-lg bg-surface-2 p-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Connaissances de l’unité">
          <Select value={source} onChange={(e) => setSource(e.target.value)}>
            {options.map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Partagées avec">
          <Select value={target} onChange={(e) => setTarget(e.target.value)}>
            {options.map((o) => (
              <option key={o.id} value={o.id}>
                {o.label}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Jusqu’au niveau de sensibilité">
          <Select value={ceiling} onChange={(e) => setCeiling(e.target.value as Sensitivity)}>
            {SENSITIVITIES.map((s) => (
              <option key={s} value={s}>
                {CLEARANCE_LABELS[s]}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Jusqu’au (facultatif)">
          <Input type="date" value={until} onChange={(e) => setUntil(e.target.value)} />
        </Field>
      </div>
      <fieldset className="flex flex-col gap-1 text-sm">
        <legend className="mb-1 text-xs font-medium text-muted">Ce qui est partagé</legend>
        <label className="flex items-start gap-2">
          <Checkbox className="mt-0.5" checked={memory} onChange={() => setMemory(!memory)} />
          <span>La mémoire de son espace</span>
        </label>
        <label className="flex items-start gap-2">
          <Checkbox
            className="mt-0.5"
            checked={documents}
            onChange={() => setDocuments(!documents)}
          />
          <span>Les documents qui y sont déposés</span>
        </label>
      </fieldset>
      <label className="flex items-start gap-2 text-sm">
        <Checkbox className="mt-0.5" checked={below} onChange={() => setBelow(!below)} />
        <span>Lu aussi par les membres des unités sous celle qui reçoit le partage</span>
      </label>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button
          type="submit"
          loading={action.pending}
          disabled={(!memory && !documents) || source === target}
        >
          Partager
        </Button>
        <Button
          type="button"
          variant="ghost"
          // Closed before its change is answered, the form could be reopened
          // and the same share sent twice.
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
