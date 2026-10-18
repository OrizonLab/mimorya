/**
 * Reviews a final answer candidate: deterministic checks first (empty,
 * truncated, ungrounded identifiers), then — when enabled and success criteria
 * exist — a model-based review. A failing reviewer never blocks the answer.
 */
import type { Citation } from '@mimorya/core';
import type { LlmProvider, LlmResponse } from '../llm/types';
import type { ToolRegistry } from '../tools/registry';
import { checkGrounding, judgeAnswer, type VerificationReport } from './verification';

export interface AnswerReviewInput {
  answer: string;
  truncated: boolean;
  evidence: Citation[];
  registry: ToolRegistry;
  objective: string;
  successCriteria: string[];
  /** Reviewer model; when absent the model-based review is skipped. */
  judge: LlmProvider | null;
  onJudgeResponse: (response: LlmResponse) => Promise<void>;
}

export async function reviewAnswer(input: AnswerReviewInput): Promise<VerificationReport> {
  const issues: string[] = [];
  if (!input.answer) issues.push('La réponse est vide.');
  if (input.truncated) {
    issues.push('La réponse a été tronquée : fournis une version plus concise.');
  }
  const patterns = input.registry
    .all()
    .map((t) => t.citationPattern)
    .filter((p): p is RegExp => !!p);
  issues.push(...checkGrounding(input.answer, patterns, input.evidence));

  let judged = false;
  if (issues.length === 0 && input.judge && input.successCriteria.length > 0) {
    try {
      const { verdict, response } = await judgeAnswer(input.judge, {
        objective: input.objective,
        criteria: input.successCriteria,
        answer: input.answer,
        evidence: input.evidence,
      });
      await input.onJudgeResponse(response);
      if (verdict) {
        judged = true;
        if (!verdict.pass) {
          issues.push(...(verdict.issues.length ? verdict.issues : ['Critères non satisfaits.']));
        }
      }
    } catch {
      // The report says the answer was not judged.
    }
  }
  return { passed: issues.length === 0, issues, judged };
}

/** Final text shown to the user: residual issues are never hidden. */
export function presentAnswer(answer: string, report: VerificationReport): string {
  if (report.passed) return answer;
  const issues = report.issues.map((i) => `- ${i}`).join('\n');
  return `${answer}\n\n---\n⚠️ **Points non vérifiés par le contrôle qualité :**\n${issues}`;
}
