'use client';

import { Loaded } from '@/components/ui';
import { useApiData, type LlmView } from '@/lib/api';
import { ConversationModel } from './conversation-model';
import { EmbeddingSection } from './embedding-section';
import { EvaluationSection } from './evaluation-section';
import { LessonSection } from './lesson-section';
import { MissionSection } from './mission-section';
import { ReviewModel } from './review-model';

/** The models the company's assistant uses: for its conversations, the
 * platform's or its own, and for reviewing their answers, one apart if it
 * chooses (ADR-108); for searching its memory by meaning, its own if any
 * (ADR-074); whether the first rereads finished conversations for a lesson
 * (ADR-076); how far a mission may go (ADR-093); and what the skill
 * evaluations may cost over thirty days (ADR-154). */
export function LlmTab() {
  return (
    <div className="flex flex-col gap-6">
      <Models />
      <EmbeddingSection />
      <LessonSection />
      <MissionSection />
      <EvaluationSection />
    </div>
  );
}

/** The model of the conversations and the one reviewing their answers, read together. */
function Models() {
  const state = useApiData<LlmView>('/admin/llm');
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-6">
          <ConversationModel data={data} onChanged={state.reload} />
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
