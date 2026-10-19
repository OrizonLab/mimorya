'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Select } from '@/components/ui';
import { api, useAction, type LlmChoice, type LlmView } from '@/lib/api';
import { LLM_PROVIDERS } from '@/lib/labels';

/** Chooses the company's own provider; the key is write-only. */
export function LlmForm({ data, onDone }: { data: LlmView; onDone: (changed: boolean) => void }) {
  const current = data.settings;
  const [provider, setProvider] = useState(current?.provider ?? 'anthropic');
  const [model, setModel] = useState(current?.model ?? '');
  const [baseUrl, setBaseUrl] = useState(current?.baseUrl ?? data.endpoints[0] ?? '');
  const [apiKey, setApiKey] = useState('');
  const action = useAction();
  const kind = data.providers.find((p) => p.kind === provider);
  const listed = kind?.listedEndpoint ?? false;
  // A stored key only ever serves the same endpoint.
  const keepsKey =
    current?.keyStored === true &&
    current.provider === provider &&
    (current.baseUrl ?? '') === (listed ? baseUrl : '');

  async function submit(event: FormEvent) {
    event.preventDefault();
    // An absent field is left out of the JSON.
    const body = {
      provider,
      model: model.trim() || undefined,
      baseUrl: listed ? baseUrl : undefined,
      apiKey: apiKey.trim() || undefined,
    } satisfies LlmChoice;
    if (await action.run(() => api.put('/admin/llm', body))) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Fournisseur">
          <Select value={provider} onChange={(e) => setProvider(e.target.value)}>
            {data.providers.map((p) => (
              <option
                key={p.kind}
                value={p.kind}
                disabled={
                  (p.listedEndpoint && data.endpoints.length === 0) ||
                  (p.requiresKey && !data.secretsAvailable)
                }
              >
                {LLM_PROVIDERS[p.kind] ?? p.kind}
              </option>
            ))}
          </Select>
        </Field>
        <Field label="Modèle">
          <Input
            maxLength={100}
            required={!kind?.defaultModel}
            placeholder={kind?.defaultModel ?? 'nom du modèle chez ce fournisseur'}
            value={model}
            onChange={(e) => setModel(e.target.value)}
          />
        </Field>
        {listed && (
          <Field label="Serveur (liste de l’exploitant)">
            <Select value={baseUrl} onChange={(e) => setBaseUrl(e.target.value)}>
              {data.endpoints.map((url) => (
                <option key={url} value={url}>
                  {url}
                </option>
              ))}
            </Select>
          </Field>
        )}
        {(kind?.requiresKey || data.secretsAvailable) && (
          <Field
            label={
              keepsKey
                ? 'Clé d’API (laisser vide pour conserver la clé enregistrée)'
                : kind?.requiresKey
                  ? 'Clé d’API'
                  : 'Clé d’API (facultative)'
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
        La clé est chiffrée sur le serveur et ne s’affiche plus jamais. Elle ne sert qu’à ce
        fournisseur, à cette adresse : en changer demande de la saisir de nouveau.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          Enregistrer
        </Button>
        <Button type="button" variant="ghost" onClick={() => onDone(false)}>
          Annuler
        </Button>
      </div>
    </form>
  );
}
