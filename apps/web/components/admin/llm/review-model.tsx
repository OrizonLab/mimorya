'use client';

import { useState } from 'react';
import { Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type LlmTestResult, type LlmView } from '@/lib/api';
import { LlmForm } from './llm-form';
import { ModelSummary, TestOutcome } from './model-summary';

const PATH = '/admin/llm/verification';

/**
 * The model the company chooses apart to review its assistant's answers
 * (ADR-108): on the quality criteria of a skill followed, of a mission or of
 * an evaluation case. Without it, or while it cannot be used, the model of
 * its conversations reviews them.
 */
export function ReviewModel({ data, onChanged }: { data: LlmView; onChanged: () => void }) {
  const [editing, setEditing] = useState(false);
  const [tested, setTested] = useState<LlmTestResult | null>(null);
  const action = useAction();
  const chosen = data.verification;

  async function test() {
    setTested(await action.run(() => api.post<LlmTestResult>(`${PATH}/test`)));
  }

  async function reset() {
    const asked =
      'Faire relire les réponses par le modèle de vos conversations ? La clé du modèle de contrôle sera effacée.';
    if (!window.confirm(asked)) return;
    if (await action.run(() => api.delete(PATH))) {
      setTested(null);
      onChanged();
    }
  }

  return (
    <section aria-label="Modèle de contrôle des réponses" className="flex flex-col gap-4">
      <Card className="flex flex-col gap-3 p-4">
        <p className="font-medium">Modèle de contrôle des réponses</p>
        {chosen ? (
          <ModelSummary
            title="Modèle de contrôle"
            settings={chosen}
            unusable="Le modèle de vos conversations relit les réponses en attendant"
          />
        ) : (
          <p className="text-sm">Les réponses sont relues par le modèle de vos conversations.</p>
        )}
        {tested && <TestOutcome tested={tested} />}
        {action.error && <ErrorNote message={action.error} />}
        <div className="flex flex-wrap gap-2">
          {!editing && (
            <Button variant="secondary" onClick={() => setEditing(true)}>
              {chosen ? 'Modifier le modèle de contrôle' : 'Choisir un modèle de contrôle'}
            </Button>
          )}
          {chosen && (
            <>
              <Button variant="secondary" loading={action.pending} onClick={test}>
                Tester le modèle de contrôle
              </Button>
              <Button variant="danger" onClick={reset}>
                Revenir au modèle des conversations
              </Button>
            </>
          )}
        </div>
      </Card>
      {editing && (
        <Card className="p-4">
          <LlmForm
            data={data}
            current={chosen}
            path={PATH}
            label="Modèle de contrôle"
            keyOf="du modèle de contrôle"
            onDone={(changed) => {
              setEditing(false);
              setTested(null);
              if (changed) onChanged();
            }}
          />
        </Card>
      )}
      <p className="text-xs text-muted">
        Un modèle relit la réponse de l’assistant quand elle a des critères à tenir : ceux d’une
        compétence suivie, d’une mission ou d’un cas d’évaluation. Un modèle choisi à part, d’un
        autre fournisseur ou moins coûteux, juge sans avoir écrit la réponse. Il reçoit la demande,
        la réponse et les sources consultées : vérifiez qu’il convient à vos données.
      </p>
    </section>
  );
}
