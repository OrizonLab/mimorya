'use client';

import { Badge, Loaded } from '@/components/ui';
import { useApiData, type SkillEvaluationDetail } from '@/lib/api';
import { CaseResult } from './case-result';

/**
 * An evaluation case by case (ADR-104): each case's outcome for the
 * version evaluated and for the active one beside it, regressions marked.
 */
export function EvaluationDetail({ id }: { id: string }) {
  const state = useApiData<SkillEvaluationDetail>(`/admin/skill-evaluations/${id}`);
  return (
    <Loaded state={state}>
      {(detail) => {
        const { evaluation } = detail;
        return (
          <div className="mt-2 flex flex-col gap-2">
            {!detail.detailsShown && (
              <p className="text-xs text-muted">
                Les réponses, les manquements relevés et les outils appelés ne sont lus que par la
                personne qui a demandé l’évaluation : elle a tourné avec ses droits.
              </p>
            )}
            <ol className="flex flex-col gap-2">
              {detail.cases.map((item, index) => (
                <li key={`${index} ${item.name}`} className="rounded-lg bg-surface p-3 text-sm">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="font-medium">{item.name}</span>
                    {item.regression && <Badge tone="danger">Régression</Badge>}
                  </div>
                  <p className="mt-1 whitespace-pre-wrap text-xs text-muted">{item.request}</p>
                  <div className="mt-2 flex flex-col gap-2">
                    <CaseResult label={`Version ${evaluation.version}`} result={item.result} />
                    {evaluation.referenceVersion !== null && (
                      <CaseResult
                        label={`Version active ${evaluation.referenceVersion}`}
                        result={item.reference}
                      />
                    )}
                  </div>
                </li>
              ))}
            </ol>
          </div>
        );
      }}
    </Loaded>
  );
}
