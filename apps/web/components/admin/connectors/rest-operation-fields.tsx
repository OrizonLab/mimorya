'use client';

import { Field, Input, Select, Textarea, Button } from '@/components/ui';
import type { HttpMethod } from '@/lib/api';
import { METHODS, type OperationDraft } from './rest-drafts';
import { RestParameters } from './rest-parameters';

/**
 * One operation of a REST API: the tool it becomes (`rest_<prefix>_<name>`),
 * the request it sends and what the model reads of it.
 */
export function RestOperationFields({
  operation,
  index,
  prefix,
  onChange,
  onRemove,
}: {
  operation: OperationDraft;
  index: number;
  prefix: string;
  onChange: (operation: OperationDraft) => void;
  onRemove?: () => void;
}) {
  const set = <K extends keyof OperationDraft>(key: K, value: OperationDraft[K]) =>
    onChange({ ...operation, [key]: value });

  return (
    <fieldset className="flex flex-col gap-3 rounded-lg bg-surface-2 p-3">
      <legend className="px-1 text-sm font-medium">Opération {index + 1}</legend>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Nom de l’opération">
          <Input
            required
            maxLength={40}
            pattern="[a-z][a-z0-9_]{1,39}"
            placeholder="get_ticket"
            value={operation.name}
            onChange={(e) => set('name', e.target.value)}
          />
        </Field>
        <Field label="Intitulé">
          <Input
            required
            maxLength={120}
            placeholder="Lire un ticket"
            value={operation.title}
            onChange={(e) => set('title', e.target.value)}
          />
        </Field>
        <Field label="Méthode">
          <Select
            value={operation.method}
            onChange={(e) => set('method', e.target.value as HttpMethod)}
          >
            {METHODS.map((method) => (
              <option key={method} value={method}>
                {method}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Chemin (après l’adresse de l’API)">
          <Input
            required
            maxLength={500}
            placeholder="/tickets/{id}"
            value={operation.path}
            onChange={(e) => set('path', e.target.value)}
          />
        </Field>
      </div>
      <Field label="Description lue par le modèle">
        <Textarea
          required
          rows={2}
          maxLength={1000}
          placeholder="Renvoie un ticket et son état."
          value={operation.description}
          onChange={(e) => set('description', e.target.value)}
        />
      </Field>
      <p className="text-xs text-muted">
        Outil{' '}
        <code>
          rest_{prefix || '…'}_{operation.name || '…'}
        </code>
        . Un paramètre de chemin s’écrit {'{nom}'} dans le chemin ; sa valeur est encodée entière.
      </p>
      <RestParameters
        parameters={operation.parameters}
        onChange={(parameters) => set('parameters', parameters)}
      />
      {onRemove && (
        <Button type="button" variant="ghost" className="w-fit" onClick={onRemove}>
          Retirer l’opération {index + 1}
        </Button>
      )}
    </fieldset>
  );
}
