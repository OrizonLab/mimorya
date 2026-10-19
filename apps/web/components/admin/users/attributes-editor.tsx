'use client';

import { Plus, X } from 'lucide-react';
import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Select } from '@/components/ui';
import { api, useAction, type AccountUpdate, type DirectoryUser } from '@/lib/api';
import { useSyncedState } from '@/lib/use-synced-state';
import {
  ATTRIBUTE_KINDS,
  fromRows,
  toRows,
  type AttributeKind,
  type AttributeRow,
} from './attributes';

const TEXTAREA =
  'rounded-lg border border-line bg-surface px-3 py-2 text-sm text-fg outline-none focus:border-accent';

/** The attributes access policies read (`subject.attributes.<nom>`). */
export function AttributesEditor({
  user,
  reloading,
  onChanged,
}: {
  user: DirectoryUser;
  reloading: boolean;
  onChanged: () => void;
}) {
  const initial = toRows(user.attributes);
  if (!initial) {
    return (
      <div className="flex flex-col gap-2 text-sm">
        <p className="text-muted">
          Ces attributs ont été écrits hors de l’interface : ils ne se modifient pas ici, pour ne
          pas les altérer.
        </p>
        <pre className="overflow-x-auto rounded-lg bg-surface p-2 text-xs">
          {JSON.stringify(user.attributes, null, 2)}
        </pre>
      </div>
    );
  }
  return <RowsEditor user={user} initial={initial} reloading={reloading} onChanged={onChanged} />;
}

function RowsEditor({
  user,
  initial,
  reloading,
  onChanged,
}: {
  user: DirectoryUser;
  initial: AttributeRow[];
  reloading: boolean;
  onChanged: () => void;
}) {
  // The attributes as the directory lists them, once more after each reload:
  // saving replaces them all.
  const [rows, setRows] = useSyncedState<AttributeRow[]>(initial);
  const [problem, setProblem] = useState<string | null>(null);
  const action = useAction();

  function update(index: number, change: Partial<AttributeRow>) {
    setRows((current) => current.map((row, i) => (i === index ? { ...row, ...change } : row)));
  }

  async function save(event: FormEvent) {
    event.preventDefault();
    const { attributes, error } = fromRows(rows);
    setProblem(error ?? null);
    if (!attributes) return;
    const update = { attributes } satisfies AccountUpdate;
    const saved = await action.run(() => api.patch(`/admin/users/${user.id}`, update));
    if (saved) onChanged();
  }

  return (
    <form onSubmit={save}>
      {/* Until the directory is reloaded, nothing is changed from former attributes. */}
      <fieldset disabled={reloading} className="flex flex-col gap-3">
        <p className="text-xs text-muted">
          Les politiques d’accès les lisent sous <code>subject.attributes.nom</code> (par exemple «
          department » = « administratif »). Nom : une minuscule puis des lettres ou des chiffres.
          N’y placez aucune donnée sensible (santé, opinions, vie privée), ni dans la valeur ni dans
          le nom : le nom reste visible dans le journal d’audit et l’éditeur de politiques.
        </p>
        {rows.length === 0 && <p className="text-sm text-muted">Aucun attribut.</p>}
        {rows.map((row, index) => (
          <div key={index} className="grid gap-2 sm:grid-cols-[1fr_10rem_2fr_auto] sm:items-end">
            <Field label="Nom">
              <Input
                required
                maxLength={40}
                pattern="[a-z][a-zA-Z0-9]*"
                value={row.key}
                onChange={(e) => update(index, { key: e.target.value })}
              />
            </Field>
            <Field label="Type">
              <Select
                value={row.kind}
                onChange={(e) => {
                  const kind = e.target.value as AttributeKind;
                  // A yes/no value is always one of the two.
                  const value = kind === 'boolean' && row.value !== 'false' ? 'true' : row.value;
                  update(index, { kind, value });
                }}
              >
                {Object.entries(ATTRIBUTE_KINDS).map(([kind, label]) => (
                  <option key={kind} value={kind}>
                    {label}
                  </option>
                ))}
              </Select>
            </Field>
            <Field label={row.kind === 'list' ? 'Valeurs (une par ligne)' : 'Valeur'}>
              {row.kind === 'boolean' ? (
                <Select
                  value={row.value}
                  onChange={(e) => update(index, { value: e.target.value })}
                >
                  <option value="true">Oui</option>
                  <option value="false">Non</option>
                </Select>
              ) : row.kind === 'list' ? (
                <textarea
                  required
                  rows={2}
                  className={TEXTAREA}
                  value={row.value}
                  onChange={(e) => update(index, { value: e.target.value })}
                />
              ) : (
                <Input
                  required
                  type={row.kind === 'number' ? 'number' : 'text'}
                  step="any"
                  maxLength={200}
                  value={row.value}
                  onChange={(e) => update(index, { value: e.target.value })}
                />
              )}
            </Field>
            <Button
              type="button"
              variant="ghost"
              onClick={() => setRows((current) => current.filter((_, i) => i !== index))}
            >
              <X size={16} aria-hidden />
              <span className="sr-only">Retirer l’attribut {row.key}</span>
            </Button>
          </div>
        ))}
        {problem && <ErrorNote message={problem} />}
        {action.error && <ErrorNote message={action.error} />}
        <div className="flex flex-wrap gap-2">
          <Button
            type="button"
            variant="secondary"
            disabled={rows.length >= 20}
            onClick={() => setRows((current) => [...current, { key: '', kind: 'text', value: '' }])}
          >
            <Plus size={16} aria-hidden />
            Ajouter un attribut
          </Button>
          <Button type="submit" loading={action.pending}>
            Enregistrer les attributs
          </Button>
        </div>
      </fieldset>
    </form>
  );
}
