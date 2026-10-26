'use client';

import { Plus, X } from 'lucide-react';
import { Button, Checkbox, Field, Input, Select } from '@/components/ui';
import type { ParameterLocation, ParameterType } from '@/lib/api';
import { KINDS, LOCATIONS, emptyParameter, type ParameterDraft } from './rest-drafts';

/**
 * The parameters of one operation: where each goes (path, query, JSON
 * body), its type, whether it is required (a path parameter always is) and
 * what the model reads of it.
 */
export function RestParameters({
  parameters,
  onChange,
}: {
  parameters: ParameterDraft[];
  onChange: (parameters: ParameterDraft[]) => void;
}) {
  function change(key: number, patch: Partial<ParameterDraft>) {
    onChange(parameters.map((p) => (p.key === key ? { ...p, ...patch } : p)));
  }

  return (
    <div className="flex flex-col gap-2">
      {parameters.map((parameter, index) => (
        <fieldset
          key={parameter.key}
          className="grid gap-2 rounded-lg border border-line p-2 sm:grid-cols-[1fr_9rem_8rem_auto]"
        >
          <legend className="px-1 text-xs text-muted">Paramètre {index + 1}</legend>
          <Field label="Nom">
            <Input
              required
              maxLength={64}
              pattern="[A-Za-z][A-Za-z0-9_.\-]*"
              placeholder="id"
              value={parameter.name}
              onChange={(e) => change(parameter.key, { name: e.target.value })}
            />
          </Field>
          <Field label="Emplacement">
            <Select
              value={parameter.location}
              onChange={(e) =>
                change(parameter.key, { location: e.target.value as ParameterLocation })
              }
            >
              {Object.entries(LOCATIONS).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </Select>
          </Field>
          <Field label="Type">
            <Select
              value={parameter.kind}
              onChange={(e) => change(parameter.key, { kind: e.target.value as ParameterType })}
            >
              {Object.entries(KINDS).map(([value, label]) => (
                <option key={value} value={value}>
                  {label}
                </option>
              ))}
            </Select>
          </Field>
          <label className="flex items-center gap-1 self-end pb-2 text-xs text-muted">
            <Checkbox
              checked={parameter.required || parameter.location === 'path'}
              disabled={parameter.location === 'path'}
              onChange={(e) => change(parameter.key, { required: e.target.checked })}
            />
            Requis
          </label>
          <Field label="Description (facultative)" className="sm:col-span-3">
            <Input
              maxLength={300}
              value={parameter.description}
              onChange={(e) => change(parameter.key, { description: e.target.value })}
            />
          </Field>
          <Button
            type="button"
            variant="ghost"
            className="self-end"
            aria-label={`Retirer le paramètre ${index + 1}`}
            onClick={() => onChange(parameters.filter((p) => p.key !== parameter.key))}
          >
            <X className="size-4" aria-hidden />
          </Button>
        </fieldset>
      ))}
      <Button
        type="button"
        variant="ghost"
        className="w-fit"
        onClick={() => onChange([...parameters, emptyParameter()])}
      >
        <Plus className="size-4" aria-hidden /> Ajouter un paramètre
      </Button>
    </div>
  );
}
