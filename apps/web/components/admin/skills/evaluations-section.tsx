'use client';

import { Loaded } from '@/components/ui';
import { useApiData, type SkillEvaluations, type SkillVersionView } from '@/lib/api';
import { formatCost } from '@/lib/format';
import { useRefreshWhile } from '@/lib/use-refresh-while';
import { EvaluationItem } from './evaluation-item';
import { EvaluationLaunch } from './evaluation-launch';

/** How often an evaluation under way is read again. */
const REFRESH_MS = 3_000;

/**
 * A skill's evaluations, the latest first (ADR-104): one asked for here,
 * read again while it is played, its results and regressions shown, with
 * what the company's evaluations cost over thirty days and their limit
 * (ADR-154).
 */
export function EvaluationsSection({
  skillId,
  versions,
  activeVersion,
  cases,
}: {
  skillId: string;
  versions: SkillVersionView[];
  activeVersion: number | null;
  cases: number;
}) {
  const state = useApiData<SkillEvaluations>(`/admin/skills/${skillId}/evaluations`);
  const latest = state.data?.evaluations[0];
  const underWay = latest?.status === 'queued' || latest?.status === 'running';
  useRefreshWhile(underWay && !state.loading, state.reload, REFRESH_MS);

  return (
    <section className="flex flex-col gap-2" aria-label="Évaluations">
      <h3 className="text-sm font-medium">Évaluations</h3>
      {cases === 0 ? (
        <p className="text-xs text-muted">Écrivez au moins un cas pour évaluer une version.</p>
      ) : (
        <EvaluationLaunch
          // A version written since: chosen by default.
          key={versions[0]?.version}
          skillId={skillId}
          versions={versions}
          activeVersion={activeVersion}
          underWay={underWay}
          onLaunched={state.reload}
        />
      )}
      <Loaded state={state}>
        {(data) => (
          <div className="flex flex-col gap-2">
            {data.evaluations.length === 0 ? (
              <p className="text-xs text-muted">Aucune évaluation.</p>
            ) : (
              <ol className="flex flex-col gap-2">
                {data.evaluations.map((evaluation) => (
                  <EvaluationItem key={evaluation.id} evaluation={evaluation} />
                ))}
              </ol>
            )}
            <p className="text-xs">
              Évaluations de l’entreprise sur trente jours : {formatCost(data.spentMicros)}{' '}
              dépensés, pour une limite de {formatCost(data.maxCostMicros)}.
            </p>
          </div>
        )}
      </Loaded>
      <p className="text-xs text-muted">
        Une évaluation joue chaque cas pour la version choisie et, à côté, pour la version active :
        l’assistant répond en lecture seule, avec vos droits, puis un modèle juge sa réponse sur les
        critères du cas. Un cas réussi par la version active et manqué par l’autre est une
        régression : activer cette version demandera une confirmation. Une fois la limite de coût
        atteinte (réglée dans « Modèle d’IA »), aucune évaluation ne peut être demandée.
      </p>
    </section>
  );
}
