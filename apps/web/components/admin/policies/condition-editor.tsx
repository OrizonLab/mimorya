'use client';

import { Plus, X } from 'lucide-react';
import { Button, cx, DISABLED_LOOK, Field, Input, Select } from '@/components/ui';
import type { Policies } from '@/lib/api';
import {
  OPERATORS,
  fromSimple,
  toSimple,
  type ConditionJson,
  type Operator,
  type SimpleCondition,
} from './types';

export type EditorState =
  { mode: 'simple'; simple: SimpleCondition } | { mode: 'json'; json: string; error?: string };

const ATTRIBUTE_LIST = 'policy-attributes';
const FREE_PREFIXES = ['subject.attributes.', 'resource.attributes.', 'context.'];

export function initialEditor(condition?: ConditionJson): EditorState {
  if (!condition) {
    return {
      mode: 'simple',
      simple: { mode: 'all', rows: [{ attr: 'resource.type', op: 'eq', value: '' }] },
    };
  }
  const simple = toSimple(condition);
  return simple
    ? { mode: 'simple', simple }
    : { mode: 'json', json: JSON.stringify(condition, null, 2) };
}

/** The condition to send, or why it cannot be sent yet. */
export function editorCondition(state: EditorState): { condition?: ConditionJson; error?: string } {
  if (state.mode === 'simple') return { condition: fromSimple(state.simple) };
  try {
    const parsed: unknown = JSON.parse(state.json);
    if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
      return { condition: parsed as ConditionJson };
    }
  } catch {
    // reported below
  }
  return { error: 'La condition n’est pas un objet JSON valide.' };
}

/** Rows joined by « et » / « ou », or the JSON of a nested condition. */
export function ConditionEditor({
  value,
  attributes,
  onChange,
}: {
  value: EditorState;
  attributes: Policies['attributes'];
  onChange: (state: EditorState) => void;
}) {
  function toJson() {
    const { condition } = editorCondition(value);
    onChange({ mode: 'json', json: JSON.stringify(condition, null, 2) });
  }

  function toRows() {
    if (value.mode !== 'json') return;
    const { condition, error } = editorCondition(value);
    const simple = condition ? toSimple(condition) : null;
    if (simple) onChange({ mode: 'simple', simple });
    else onChange({ ...value, error: error ?? 'Condition trop imbriquée pour l’éditeur simple.' });
  }

  const datalist = (
    <datalist id={ATTRIBUTE_LIST}>
      {attributes.map((a) => (
        <option key={a.path} value={a.path}>
          {a.label}
        </option>
      ))}
      {FREE_PREFIXES.map((prefix) => (
        <option key={prefix} value={prefix} />
      ))}
    </datalist>
  );

  if (value.mode === 'json') {
    return (
      <div className="flex flex-col gap-2">
        <Field label="Condition (JSON)">
          <textarea
            className={cx(
              'min-h-40 rounded-lg border border-line bg-surface p-2 font-mono text-xs',
              DISABLED_LOOK,
            )}
            value={value.json}
            onChange={(e) => onChange({ mode: 'json', json: e.target.value })}
          />
        </Field>
        {value.error && <p className="text-xs text-danger">{value.error}</p>}
        <Button type="button" variant="ghost" className="w-fit" onClick={toRows}>
          Revenir à l’éditeur simple
        </Button>
      </div>
    );
  }

  const { simple } = value;
  const setSimple = (next: SimpleCondition) => onChange({ mode: 'simple', simple: next });
  const setRow = (index: number, patch: Partial<SimpleCondition['rows'][number]>) =>
    setSimple({
      ...simple,
      rows: simple.rows.map((r, i) => (i === index ? { ...r, ...patch } : r)),
    });

  return (
    <div className="flex flex-col gap-2">
      {datalist}
      <Field label="La politique s’applique quand">
        <Select
          value={simple.mode}
          onChange={(e) => setSimple({ ...simple, mode: e.target.value as 'all' | 'any' })}
        >
          <option value="all">toutes les conditions sont remplies</option>
          <option value="any">au moins une condition est remplie</option>
        </Select>
      </Field>
      {simple.rows.map((row, index) => (
        <div key={index} className="grid items-end gap-2 sm:grid-cols-[2fr_1.4fr_2fr_auto]">
          <Field label="Attribut">
            <Input
              required
              list={ATTRIBUTE_LIST}
              value={row.attr}
              onChange={(e) => setRow(index, { attr: e.target.value })}
            />
          </Field>
          <Field label="Comparaison">
            <Select
              value={row.op}
              onChange={(e) => setRow(index, { op: e.target.value as Operator })}
            >
              {Object.entries(OPERATORS).map(([op, label]) => (
                <option key={op} value={op}>
                  {label}
                </option>
              ))}
            </Select>
          </Field>
          <Field
            label={
              row.op === 'in' || row.op === 'not_in'
                ? 'Valeurs (séparées par des virgules)'
                : 'Valeur'
            }
          >
            <Input
              disabled={row.op === 'exists'}
              value={row.op === 'exists' ? '' : row.value}
              onChange={(e) => setRow(index, { value: e.target.value })}
            />
          </Field>
          <Button
            type="button"
            variant="ghost"
            className="px-2 py-2"
            disabled={simple.rows.length === 1}
            onClick={() =>
              setSimple({ ...simple, rows: simple.rows.filter((_, i) => i !== index) })
            }
          >
            <X className="size-4" aria-hidden />
            <span className="sr-only">Retirer cette condition</span>
          </Button>
        </div>
      ))}
      <div className="flex flex-wrap gap-2">
        <Button
          type="button"
          variant="ghost"
          onClick={() =>
            setSimple({ ...simple, rows: [...simple.rows, { attr: '', op: 'eq', value: '' }] })
          }
        >
          <Plus className="size-4" aria-hidden /> Ajouter une condition
        </Button>
        <Button type="button" variant="ghost" onClick={toJson}>
          Mode avancé (JSON)
        </Button>
      </div>
    </div>
  );
}
