'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction, type LlmChoice, type LlmView } from '@/lib/api';
import { LLM_PROVIDERS } from '@/lib/labels';
import { EndpointSelect } from './endpoint-select';
import { KeyField, keepsStoredKey } from './key-field';
import { ProviderSelect } from './provider-select';

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
  const keepsKey = keepsStoredKey(current, provider, listed ? baseUrl : '');

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
    <form
      aria-label="Modèle de vos conversations"
      onSubmit={submit}
      className="flex flex-col gap-3"
    >
      <div className="grid gap-3 sm:grid-cols-2">
        <ProviderSelect
          label="Fournisseur"
          names={LLM_PROVIDERS}
          providers={data.providers}
          endpoints={data.endpoints}
          secretsAvailable={data.secretsAvailable}
          value={provider}
          onChange={setProvider}
        />
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
          <EndpointSelect
            label="Serveur (liste de l’exploitant)"
            endpoints={data.endpoints}
            value={baseUrl}
            onChange={setBaseUrl}
          />
        )}
        {(kind?.requiresKey || data.secretsAvailable) && (
          <KeyField
            required={kind?.requiresKey ?? false}
            keeps={keepsKey}
            value={apiKey}
            onChange={setApiKey}
          />
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
        <Button
          type="button"
          variant="ghost"
          // Closed before its change is answered, it could be reopened from the
          // former resource, and that change overwritten.
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
