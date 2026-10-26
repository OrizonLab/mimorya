'use client';

import { useState } from 'react';
import { Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type LlmTestResult, type LlmView } from '@/lib/api';
import { LlmForm } from './llm-form';
import { ModelSummary, TestOutcome, providerLabel } from './model-summary';

/** The model the company's conversations use: the platform's or its own (ADR-024). */
export function ConversationModel({ data, onChanged }: { data: LlmView; onChanged: () => void }) {
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
      onChanged();
    }
  }

  return (
    <div className="flex flex-col gap-4">
      <Card className="flex flex-col gap-3 p-4">
        <p className="font-medium">Modèle utilisé par vos conversations</p>
        {data.settings ? (
          <>
            <ModelSummary
              title="Fournisseur de votre entreprise"
              settings={data.settings}
              unusable="Vos conversations sont arrêtées"
            />
            <p className="text-xs text-muted">
              Vos conversations n’utilisent que ce fournisseur : s’il devient inutilisable, elles
              s’arrêtent plutôt que de passer par celui de la plateforme.
            </p>
          </>
        ) : (
          <p className="text-sm">
            {data.platform
              ? `Modèle de la plateforme : ${providerLabel(data.platform.provider)} · ${data.platform.model}`
              : 'La plateforme ne fournit pas de modèle : configurez celui de votre entreprise.'}
          </p>
        )}
        {tested && <TestOutcome tested={tested} />}
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
            current={data.settings}
            path="/admin/llm"
            label="Modèle de vos conversations"
            onDone={(changed) => {
              setEditing(false);
              setTested(null);
              if (changed) onChanged();
            }}
          />
        </Card>
      )}
    </div>
  );
}
