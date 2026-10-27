'use client';

import { useState } from 'react';
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
  // A new key empties the file input once its description is read.
  const [picker, setPicker] = useState(0);
  const action = useAction();

  async function read(file: File | undefined) {
    if (!file) return;
    setOffered(null);
    setChosen(new Set());
    const read = await action.run(async () => {
      const document = await descriptionDocument(file);
      return api.post<RestImport>('/admin/connectors/rest/import', {
        document,
      } satisfies OpenApiDescription);
    });
    setPicker((n) => n + 1);
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
    onAdd(
      offered.operations.filter((operation) => chosen.has(operation.name)),
      offered.url,
    );
    setOffered(null);
    setChosen(new Set());
  }

  return (
    <section
      aria-labelledby="openapi-import"
      className="flex flex-col gap-2 rounded-lg border border-line p-3"
    >
      <h3 id="openapi-import" className="text-sm font-medium">
        Importer une description OpenAPI
      </h3>
      <p className="text-xs text-muted">
        OpenAPI 3, en JSON ou en YAML (converti dans ce navigateur), 768 Kio au plus. La plateforme
        n’appelle rien et n’enregistre rien : cochez les opérations à ajouter, puis relisez-les dans
        le formulaire avant de l’enregistrer.
      </p>
      <Field label="Fichier de description">
        <Input
          key={picker}
          type="file"
          accept=".json,.yaml,.yml,application/json,application/yaml"
          disabled={action.pending}
          onChange={(e) => read(e.target.files?.[0])}
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
              const checked = chosen.has(operation.name);
              const full = !checked && chosen.size >= room;
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
            <Button type="button" disabled={chosen.size === 0} onClick={add}>
              Ajouter les opérations cochées ({chosen.size})
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
