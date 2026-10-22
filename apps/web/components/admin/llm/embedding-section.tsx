'use client';

import { useState } from 'react';
import { Badge, Button, Card, ErrorNote, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type EmbeddingTestResult,
  type EmbeddingView,
} from '@/lib/api';
import { formatDate } from '@/lib/format';
import { EMBEDDING_PROVIDERS } from '@/lib/labels';
import { EmbeddingForm } from './embedding-form';

const label = (provider: string) => EMBEDDING_PROVIDERS[provider] ?? provider;

/** How the company's memory is searched (ADR-074): by its words, and by its
 * meaning once an embedding model is chosen. */
export function EmbeddingSection() {
  const state = useApiData<EmbeddingView>('/admin/embedding');
  const [editing, setEditing] = useState(false);
  const [tested, setTested] = useState<EmbeddingTestResult | null>(null);
  const action = useAction();

  async function test() {
    setTested(await action.run(() => api.post<EmbeddingTestResult>('/admin/embedding/test')));
  }

  async function reset() {
    const confirmed = window.confirm(
      'Chercher dans la mémoire par les mots seulement ? Les vecteurs de votre mémoire et la ' +
        'clé enregistrée seront effacés.',
    );
    if (confirmed && (await action.run(() => api.delete('/admin/embedding')))) {
      setTested(null);
      state.reload();
    }
  }

  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-4">
          <Card className="flex flex-col gap-3 p-4">
            <p className="font-medium">Recherche dans la mémoire</p>
            {data.settings ? (
              <div className="flex flex-col gap-2 text-sm">
                <p>
                  Par les mots et par le sens : {label(data.settings.provider)} ·{' '}
                  <code>{data.settings.model}</code>
                  {data.settings.baseUrl && <> · {data.settings.baseUrl}</>} · similarité minimale{' '}
                  {data.settings.minSimilarity}
                </p>
                <div className="flex flex-wrap gap-2">
                  <Badge tone={data.settings.keyStored ? 'ok' : 'neutral'}>
                    {data.settings.keyStored ? 'Clé enregistrée' : 'Sans clé'}
                  </Badge>
                  {!data.settings.usable && <Badge tone="danger">Inutilisable</Badge>}
                </div>
                <p>
                  Mémoires indexées : {data.settings.embedded} sur {data.settings.memories}
                </p>
                {data.settings.problem && (
                  <ErrorNote
                    message={`La mémoire est cherchée par les mots seulement : ${data.settings.problem}.`}
                  />
                )}
                {data.settings.indexingError && (
                  <ErrorNote
                    message={`L’indexation reprendra à la minute : ${data.settings.indexingError}`}
                  />
                )}
                <p className="text-xs text-muted">
                  Modifié le {formatDate(data.settings.updatedAt)}. Une mémoire écrite ou approuvée
                  est indexée en arrière-plan ; tant qu’elle ne l’est pas, ses mots la trouvent.
                </p>
              </div>
            ) : (
              <p className="text-sm">
                Par les mots seulement. Un modèle d’embeddings la cherche aussi par le sens : une
                demande sur un « véhicule » retrouve la mémoire qui parle des « camionnettes ».
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
                  {data.settings ? 'Modifier' : 'Chercher aussi par le sens'}
                </Button>
              )}
              {data.settings && (
                <>
                  <Button variant="secondary" loading={action.pending} onClick={test}>
                    Tester le modèle d’embeddings
                  </Button>
                  <Button variant="secondary" onClick={() => state.reload()}>
                    Actualiser
                  </Button>
                  <Button variant="danger" onClick={reset}>
                    Chercher par les mots seulement
                  </Button>
                </>
              )}
            </div>
          </Card>
          {editing && (
            <Card className="p-4">
              <EmbeddingForm
                data={data}
                onDone={(changed) => {
                  setEditing(false);
                  setTested(null);
                  if (changed) state.reload();
                }}
              />
            </Card>
          )}
          <p className="text-xs text-muted">
            Choisir un modèle envoie au fournisseur le texte des mémoires actives de votre
            entreprise et celui des demandes : vérifiez qu’il convient à vos données. Revenir aux
            mots efface tous les vecteurs. Chaque appel est compté dans la consommation.
          </p>
        </div>
      )}
    </Loaded>
  );
}
