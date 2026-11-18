'use client';

import { useState } from 'react';
import { Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type LlmTestResult, type LlmView } from '@/lib/api';
import { LlmForm } from './llm-form';
import { ModelSummary, TestOutcome, providerLabel } from './model-summary';

/**
 * The model the company's conversations use: its own, or the platform's when
 * the operator supplies it (ADR-024, ADR-208). Without the platform's, its own
 * removed leaves it without any.
 */
export function ConversationModel({ data, onChanged }: { data: LlmView; onChanged: () => void }) {
  const [editing, setEditing] = useState(false);
  const [tested, setTested] = useState<LlmTestResult | null>(null);
  const action = useAction();

  async function test() {
    setTested(await action.run(() => api.post<LlmTestResult>('/admin/llm/test')));
  }

  async function reset() {
    const question = data.platform
      ? 'Revenir au modèle de la plateforme ? La clé enregistrée sera effacée.'
      : 'Retirer le fournisseur de votre entreprise ? La clé enregistrée sera effacée, et vos conversations n’auront plus de modèle.';
    if (!window.confirm(question)) return;
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
              : 'La plateforme ne fournit pas de modèle à votre entreprise : configurez le vôtre.'}
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
              {data.platform ? 'Revenir au modèle de la plateforme' : 'Retirer ce fournisseur'}
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
