'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Select } from '@/components/ui';
import { api, useAction, type EmbeddingChoice, type EmbeddingView } from '@/lib/api';
import { EMBEDDING_PROVIDERS } from '@/lib/labels';

/** Chooses the company's embedding model (ADR-074); the key is write-only. */
export function EmbeddingForm({
  data,
  onDone,
}: {
  data: EmbeddingView;
  onDone: (changed: boolean) => void;
}) {
  const current = data.settings;
  const [provider, setProvider] = useState(current?.provider ?? 'mistral');
  const [model, setModel] = useState(current?.model ?? '');
  const [baseUrl, setBaseUrl] = useState(current?.baseUrl ?? data.endpoints[0] ?? '');
  const [similarity, setSimilarity] = useState(current ? String(current.minSimilarity) : '');
  const [apiKey, setApiKey] = useState('');
  const action = useAction();
  const kind = data.providers.find((p) => p.kind === provider);
  const listed = kind?.listedEndpoint ?? false;
  // A stored key only ever serves the same endpoint.
  const keepsKey =
    current?.keyStored === true &&
    current.provider === provider &&
    (current.baseUrl ?? '') === (listed ? baseUrl : '');

  function choose(next: string) {
    setProvider(next);
    // Models spread their similarities differently: the new kind's start.
    setSimilarity('');
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    // An absent field is left out of the JSON.
    const body = {
      provider,
      model: model.trim() || undefined,
      baseUrl: listed ? baseUrl : undefined,
      minSimilarity: similarity.trim() ? Number(similarity) : undefined,
      apiKey: apiKey.trim() || undefined,
    } satisfies EmbeddingChoice;
    if (await action.run(() => api.put('/admin/embedding', body))) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Fournisseur d’embeddings">
          <Select value={provider} onChange={(e) => choose(e.target.value)}>
            {data.providers.map((p) => (
              <option
                key={p.kind}
                value={p.kind}
                disabled={
                  (p.listedEndpoint && data.endpoints.length === 0) ||
                  (p.requiresKey && !data.secretsAvailable)
                }
              >
                {EMBEDDING_PROVIDERS[p.kind] ?? p.kind}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Modèle d’embeddings">
          <Input
            maxLength={100}
            required={!kind?.defaultModel}
            placeholder={kind?.defaultModel ?? 'modèle de 1024 dimensions'}
            value={model}
            onChange={(e) => setModel(e.target.value)}
          />
        </Field>
        {listed && (
          <Field label="Serveur d’embeddings (liste de l’exploitant)">
            <Select value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)}>
              {data.endpoints.map((url) => (
                <option key={url} value={url}>
                  {url}
                </option>
              ))}
            </Select>
          </Field>
        )}
        <Field label="Similarité minimale">
          <Input
            type="number"
            min={0.05}
            max={0.95}
            step={0.05}
            placeholder={kind ? String(kind.defaultMinSimilarity) : undefined}
            value={similarity}
            onChange={(e) => setSimilarity(e.target.value)}
          />
        </Field>
        {(kind?.requiresKey || data.secretsAvailable) && (
          <Field
            label={
              keepsKey
                ? 'Clé d’API d’embeddings (laisser vide pour conserver la clé enregistrée)'
                : kind?.requiresKey
                  ? 'Clé d’API d’embeddings'
                  : 'Clé d’API d’embeddings (facultative)'
            }
          >
            <Input
              type="password"
              autoComplete="off"
              required={kind?.requiresKey && !keepsKey}
              maxLength={500}
              value={apiKey}
              onChange={(e) => setApiKey(e.target.value)}
            />
          </Field>
        )}
      </div>
      <p className="text-xs text-muted">
        Le modèle doit produire des vecteurs de 1024 dimensions. Une mémoire compte comme proche
        d’une demande à partir de la similarité minimale : la valeur proposée est un point de départ
        propre au fournisseur, à ajuster selon le modèle. La clé est chiffrée et ne s’affiche plus
        jamais ; elle ne sert qu’à ce fournisseur, à cette adresse.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          Enregistrer
        </Button>
        <Button
          type="button"
          variant="ghost"
          // Closed before its change is answered, it could be reopened from the
          // former choice, and that change overwritten.
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
