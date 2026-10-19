'use client';

import { useState } from 'react';
import { Badge, Button, Card, ErrorNote, Loaded } from '@/components/ui';
import { api, useAction, useApiData } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { LLM_PROVIDERS } from '@/lib/labels';
import { LlmForm } from './llm-form';
import type { LlmTestResult, LlmView } from './types';

const label = (provider: string) => LLM_PROVIDERS[provider] ?? provider;

/** The model the company's conversations use: the platform's or its own. */
export function LlmTab() {
  const state = useApiData<LlmView>('/admin/llm');
  const [editing, setEditing] = useState(false);
  const [tested, setTested] = useState<LlmTestResult | null>(null);
  const action = useAction();

  async function test() {
    setTested(await action.run(() => api.post<LlmTestResult>('/admin/llm/test')));
  }

  async function reset() {
    if (!window.confirm('Revenir au modèle de la plateforme ? La clé enregistrée sera effacée.')) {
      return;
    }
    if (await action.run(() => api.delete('/admin/llm'))) {
      setTested(null);
      state.reload();
    }
  }

  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-4">
          <Card className="flex flex-col gap-3 p-4">
            <p className="font-medium">Modèle utilisé par vos conversations</p>
            {data.settings ? (
              <div className="flex flex-col gap-2 text-sm">
                <p>
                  Fournisseur de votre entreprise : {label(data.settings.provider)} ·{' '}
                  <code>{data.settings.model}</code>
                  {data.settings.baseUrl && <> · {data.settings.baseUrl}</>}
                </p>
                <div className="flex flex-wrap gap-2">
                  <Badge tone={data.settings.keyStored ? 'ok' : 'neutral'}>
                    {data.settings.keyStored ? 'Clé enregistrée' : 'Sans clé'}
                  </Badge>
                  {!data.settings.priced && (
                    <Badge tone="warn">Coût non estimé : modèle absent de la grille</Badge>
                  )}
                  {!data.settings.usable && (
                    <Badge tone="danger">Inutilisable : enregistrez-le de nouveau</Badge>
                  )}
                </div>
                <p className="text-xs text-muted">
                  Modifié le {formatDate(data.settings.updatedAt)}. Vos conversations n’utilisent
                  que ce fournisseur : s’il devient inutilisable, elles s’arrêtent plutôt que de
                  passer par celui de la plateforme.
                </p>
              </div>
            ) : (
              <p className="text-sm">
                {data.platform
                  ? `Modèle de la plateforme : ${label(data.platform.provider)} · ${data.platform.model}`
                  : 'La plateforme ne fournit pas de modèle : configurez celui de votre entreprise.'}
              </p>
            )}
            {tested && (
              <p className={tested.ok ? 'text-sm text-ok' : 'text-sm text-danger'}>
                {tested.ok
                  ? `Connexion réussie : ${tested.model} a répondu.`
                  : `Échec de la connexion : ${tested.error ?? 'erreur inconnue'}`}
              </p>
            )}
            {action.error && <ErrorNote message={action.error} />}
            <div className="flex flex-wrap gap-2">
              {!editing && (
                <Button variant="secondary" onClick={() => setEditing(true)}>
                  {data.settings ? 'Modifier' : 'Utiliser le fournisseur de votre entreprise'}
                </Button>
              )}
              {(data.settings || data.platform) && (
                <Button variant="secondary" loading={action.pending} onClick={test}>
                  Tester la connexion
                </Button>
              )}
              {data.settings && (
                <Button variant="danger" onClick={reset}>
                  Revenir au modèle de la plateforme
                </Button>
              )}
            </div>
          </Card>
          {editing && (
            <Card className="p-4">
              <LlmForm
                data={data}
                onDone={(changed) => {
                  setEditing(false);
                  setTested(null);
                  if (changed) state.reload();
                }}
              />
            </Card>
          )}
          {!data.secretsAvailable && (
            <p className="text-xs text-muted">
              Ce serveur ne conserve pas de clé (l’exploitant n’a pas configuré la clé maîtresse) :
              seuls les serveurs sans clé de la liste de l’exploitant peuvent être choisis.
            </p>
          )}
          <p className="text-xs text-muted">
            Les échanges de vos conversations sont envoyés au fournisseur choisi : vérifiez qu’il
            convient à vos données (région, conditions de traitement). Le test envoie une courte
            requête, comptée dans la consommation.
          </p>
        </div>
      )}
    </Loaded>
  );
}
