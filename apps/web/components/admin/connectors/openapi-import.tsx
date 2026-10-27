'use client';

import { useId, useState } from 'react';
import { Button, Checkbox, ErrorNote, Field, Input } from '@/components/ui';
import {
  api,
  useAction,
  type OpenApiDescription,
  type RestImport,
  type RestOperation,
} from '@/lib/api';
import { descriptionDocument } from './openapi-document';

/**
 * Reads an OpenAPI description for the operations it offers (ADR-114,
 * ADR-115): the administrator checks those to add to the form, which shows
 * them like any other before anything is saved. Nothing is stored here.
 */
export function OpenApiImport({
  room,
  taken,
  onAdd,
}: {
  /** Operations the form may still hold. */
  room: number;
  /** Names of the operations the form already holds. */
  taken: Set<string>;
  onAdd: (operations: RestOperation[], url: string | null) => void;
}) {
  const [offered, setOffered] = useState<RestImport | null>(null);
  const [chosen, setChosen] = useState<Set<string>>(new Set());
  const heading = useId();
  const action = useAction();
  // What a click adds: the checked operations the form does not hold yet,
  // within its room (the form may have changed since they were checked).
  const usable = (offered?.operations ?? [])
    .filter((operation) => chosen.has(operation.name) && !taken.has(operation.name))
    .slice(0, room);

  async function read(input: HTMLInputElement) {
    const file = input.files?.[0];
    if (!file) return;
    setOffered(null);
    setChosen(new Set());
    const read = await action.run(async () => {
      const document = await descriptionDocument(file);
      return api.post<RestImport>('/admin/connectors/rest/import', {
        document,
      } satisfies OpenApiDescription);
    });
    // Emptied, not replaced: the input keeps the focus it had.
    input.value = '';
    if (read) setOffered(read);
  }

  function toggle(name: string) {
    setChosen((current) => {
      const next = new Set(current);
      if (!next.delete(name)) next.add(name);
      return next;
    });
  }

  function add() {
    if (!offered) return;
    onAdd(usable, offered.url);
    setOffered(null);
    setChosen(new Set());
  }

  return (
    <section
      aria-labelledby={heading}
      className="flex flex-col gap-2 rounded-lg border border-line p-3"
    >
      <h3 id={heading} className="text-sm font-medium">
        Importer une description OpenAPI
      </h3>
      <p className="text-xs text-muted">
        OpenAPI 3, en JSON ou en YAML (converti dans ce navigateur), 768 Kio au plus. La plateforme
        n’appelle rien et n’enregistre rien : cochez les opérations à ajouter, puis relisez-les dans
        le formulaire avant de l’enregistrer.
      </p>
      <Field label="Fichier de description">
        <Input
          type="file"
          accept=".json,.yaml,.yml,application/json,application/yaml"
          disabled={action.pending}
          onChange={(e) => read(e.currentTarget)}
        />
      </Field>
      {action.error && <ErrorNote message={action.error} />}
      {offered && (
        <div className="flex flex-col gap-2">
          <p role="status" className="text-sm">
            {offered.operations.length} opération(s) proposée(s) ; {room} de plus au plus dans ce
            formulaire.
          </p>
          <ul className="flex max-h-72 flex-col gap-1 overflow-y-auto">
            {offered.operations.map((operation) => {
              const already = taken.has(operation.name);
              // Checked as far as the form has room: beyond, shown as not.
              const checked = usable.some((kept) => kept.name === operation.name);
              const full = !checked && usable.length >= room;
              return (
                <li key={operation.name}>
                  <label className="flex items-start gap-2 text-sm">
                    <Checkbox
                      className="mt-0.5"
                      checked={checked}
                      disabled={already || full}
                      onChange={() => toggle(operation.name)}
                    />
                    <span>
                      <span className="font-mono text-xs">
                        {operation.method} {operation.path}
                      </span>{' '}
                      — {operation.title}{' '}
                      <span className="text-xs text-muted">({operation.name})</span>
                      {already && (
                        <span className="text-xs text-muted"> · déjà dans le formulaire</span>
                      )}
                    </span>
                  </label>
                </li>
              );
            })}
          </ul>
          {offered.notes.length > 0 && (
            <details className="text-xs text-muted">
              <summary>Remarques ({offered.notes.length})</summary>
              <ul className="mt-1 list-disc pl-5">
                {offered.notes.map((note, index) => (
                  <li key={index}>{note}</li>
                ))}
              </ul>
            </details>
          )}
          <div className="flex gap-2">
            <Button type="button" disabled={usable.length === 0} onClick={add}>
              Ajouter les opérations cochées ({usable.length})
            </Button>
            <Button type="button" variant="ghost" onClick={() => setOffered(null)}>
              Fermer
            </Button>
          </div>
        </div>
      )}
    </section>
  );
}
