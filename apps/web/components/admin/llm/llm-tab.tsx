'use client';

import { useState } from 'react';
import { Loaded } from '@/components/ui';
import { useApiData, type LlmView } from '@/lib/api';
import { ConversationModel } from './conversation-model';
import { DecisionSection } from './decision-section';
import { EmbeddingSection } from './embedding-section';
import { EscalationSection } from './escalation-section';
import { EvaluationLimitSection } from './evaluation-limit-section';
import { LessonSection } from './lesson-section';
import { MissionSection } from './mission-section';
import { ModelSupplySection } from './model-supply-section';
import { ReviewModel } from './review-model';

/** How the operator supplies the company's model (ADR-210), and the
 * models the company's assistant uses: for its conversations, the
 * platform's or its own, and for reviewing their answers, one apart if it
 * chooses (ADR-108); for searching its memory by meaning, its own if any
 * (ADR-074); whether the first rereads finished conversations for a lesson
 * (ADR-076); how far a mission may go (ADR-093); what the skill
 * evaluations may cost over thirty days (ADR-154); and the stronger model
 * called when needed (ADR-207), and the engine of its decisions (ADR-213),
 * both read again whenever the conversations' model changes. */
export function LlmTab() {
  const [conversations, setConversations] = useState(0);
  return (
    <div className="flex flex-col gap-6">
      <ModelSupplySection key={`supply-${conversations}`} />
      <Models onConversationsChanged={() => setConversations((n) => n + 1)} />
      <EscalationSection key={conversations} />
      <DecisionSection key={`decision-${conversations}`} />
      <EmbeddingSection />
      <LessonSection />
      <MissionSection />
      <EvaluationLimitSection />
    </div>
  );
}

/** The model of the conversations and the one reviewing their answers, read together. */
function Models({ onConversationsChanged }: { onConversationsChanged: () => void }) {
  const state = useApiData<LlmView>('/admin/llm');
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-6">
          <ConversationModel
            data={data}
            onChanged={() => {
              state.reload();
              onConversationsChanged();
            }}
          />
          <ReviewModel data={data} onChanged={state.reload} />
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
