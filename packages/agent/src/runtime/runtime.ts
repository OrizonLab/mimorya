/**
 * Agent run loop: call the model, enforce policies on every tool call, pause
 * for human validation, verify the final answer and correct it within bounds.
 * Every step is checkpointed so that a run can be resumed.
 *
 * The loop only orchestrates; the details live next to it:
 *   tool-calls.ts     policy enforcement and execution of tool calls
 *   model-call.ts     retries and usage accounting of model calls
 *   answer-review.ts  verification of the final answer
 */
import { LlmError, type LlmMessage, type LlmResponse, type LlmToolCall } from '../llm/types';
import { ToolExecutor } from '../tools/executor';
import { toToolSpec } from '../tools/registry';
import { presentAnswer, reviewAnswer } from './answer-review';
import { checkBudget } from './budget';
import { completeWithRetry, recordModelUsage } from './model-call';
import { ZERO_USAGE, type RunCheckpoint } from './state';
import { orderByRequest, ToolCallProcessor } from './tool-calls';
import type { ApprovalDecision, RunConfig, RunOutcome, RuntimeDeps } from './types';
import { correctionMessage } from './verification';

const TRUNCATED_CALL = JSON.stringify({
  erreur: 'Appel tronqué (limite de longueur atteinte), non exécuté.',
});

export class AgentRuntime {
  private readonly tools: ToolCallProcessor;

  constructor(
    private readonly deps: RuntimeDeps,
    private readonly config: RunConfig,
  ) {
    const executor = new ToolExecutor({
      engine: deps.engine,
      registry: deps.registry,
      subject: config.subject,
      actor: config.actor,
      autonomy: config.autonomy,
      runId: config.runId,
      deliverables: deps.ports.deliverables,
    });
    this.tools = new ToolCallProcessor(executor, deps.ports);
  }

  /** Starts a run. `history` holds previous turns of the conversation (text only). */
  async start(
    input: { system: string; history: LlmMessage[]; userTurn: string },
    signal?: AbortSignal,
  ): Promise<RunOutcome> {
    const state: RunCheckpoint = {
      version: 1,
      system: input.system,
      messages: [...input.history, { role: 'user', content: input.userTurn }],
      usage: { ...ZERO_USAGE },
      correctionAttempts: 0,
      evidence: [],
      elapsedMs: 0,
    };
    await this.deps.ports.events.emit({ type: 'run_started', objective: this.config.objective });
    return this.loop(state, signal);
  }

  /**
   * Continues a run paused for validation. Approved calls run with the exact
   * parameters that were validated; rejected ones are reported to the model.
   */
  async resume(
    checkpoint: RunCheckpoint,
    decisions: Record<string, ApprovalDecision>,
    signal?: AbortSignal,
  ): Promise<RunOutcome> {
    const pending = checkpoint.pending;
    if (!pending) throw new Error('Run is not waiting for approval');
    if (pending.approvals.some((a) => !decisions[a.approvalId])) {
      return { status: 'awaiting_approval', approvals: pending.approvals, checkpoint };
    }

    const state: RunCheckpoint = structuredClone(checkpoint);
    const results = [...pending.completedResults];
    for (const approval of pending.approvals) {
      const decision = decisions[approval.approvalId]!;
      await this.deps.ports.events.emit({
        type: 'approval_resolved',
        approvalId: approval.approvalId,
        approved: decision.approved,
      });
      const tool = this.deps.registry.get(approval.toolName);
      if (!tool) {
        results.push(
          await this.tools.reject(approval, 'Outil désactivé depuis la demande de validation'),
        );
      } else if (!decision.approved) {
        const comment = decision.comment ? ` : ${decision.comment}` : '';
        results.push(
          await this.tools.reject(approval, `Action refusée par l’utilisateur${comment}`),
        );
      } else {
        results.push(
          await this.tools.run(
            state,
            tool,
            approval.toolCallId,
            approval.input,
            signal,
            approval.approvalId,
          ),
        );
      }
    }

    delete state.pending;
    state.messages.push({ role: 'tool_results', results: orderByRequest(state, results) });
    await this.deps.ports.runs.checkpoint(state);
    return this.loop(state, signal);
  }

  private async loop(state: RunCheckpoint, signal?: AbortSignal): Promise<RunOutcome> {
    const { ports } = this.deps;
    const startedAt = Date.now() - state.elapsedMs;

    for (;;) {
      state.elapsedMs = Date.now() - startedAt;
      if (signal?.aborted) return this.fail(state, 'Exécution annulée');
      const budget = checkBudget(this.config.budget, state.usage, state.elapsedMs);
      if (!budget.ok) return this.fail(state, `Arrêt : ${budget.reason}`);

      const response = await this.callModel(state, signal);
      if ('error' in response) return this.fail(state, response.error);

      state.messages.push({
        role: 'assistant',
        content: response.text,
        toolCalls: response.toolCalls,
        ...(response.native ? { native: response.native } : {}),
      });

      if (response.stopReason === 'refusal') {
        return this.fail(state, 'Le modèle a refusé de traiter cette demande.');
      }

      if (response.toolCalls.length > 0) {
        const paused = await this.handleToolTurn(state, response, signal);
        if (paused) return paused;
        continue;
      }

      // Final answer candidate.
      const answer = response.text.trim();
      const report = await reviewAnswer({
        answer,
        truncated: response.stopReason === 'max_tokens',
        evidence: state.evidence,
        registry: this.deps.registry,
        objective: this.config.objective,
        successCriteria: this.config.successCriteria,
        judge: this.config.verification.llmJudge ? this.deps.router.for('verification') : null,
        onJudgeResponse: (judgeResponse) =>
          recordModelUsage(ports, this.deps.prices, state, judgeResponse),
      });
      await ports.events.emit({
        type: 'verification',
        attempt: state.correctionAttempts + 1,
        passed: report.passed,
        issues: report.issues,
      });
      const canCorrect = state.correctionAttempts < this.config.verification.maxCorrectionAttempts;
      if (!report.passed && canCorrect) {
        state.correctionAttempts += 1;
        state.messages.push({ role: 'user', content: correctionMessage(report.issues) });
        await ports.runs.checkpoint(state);
        continue;
      }

      const finalAnswer = presentAnswer(answer, report);
      state.elapsedMs = Date.now() - startedAt;
      await ports.events.emit({ type: 'assistant_message', text: finalAnswer, final: true });
      return this.conclude({
        status: 'completed',
        answer: finalAnswer,
        verification: report,
        checkpoint: state,
      });
    }
  }

  /** One model call with the tools visible to the subject; provider failures end the run. */
  private async callModel(
    state: RunCheckpoint,
    signal?: AbortSignal,
  ): Promise<LlmResponse | { error: string }> {
    const { ports, registry, engine } = this.deps;
    const { subject, actor, autonomy, effort } = this.config;
    const tools = registry.visibleTo(engine, subject, actor, autonomy);
    try {
      const response = await completeWithRetry(
        this.deps.router.for('agent'),
        {
          system: state.system,
          messages: state.messages,
          tools: tools.map(toToolSpec),
          ...(effort ? { effort } : {}),
          ...(signal ? { signal } : {}),
          onTextDelta: (text) => void ports.events.emit({ type: 'assistant_delta', text }),
        },
        { maxRetries: this.deps.maxProviderRetries ?? 2, delayMs: this.deps.retryDelayMs ?? 1000 },
      );
      state.usage.steps += 1;
      await recordModelUsage(ports, this.deps.prices, state, response);
      return response;
    } catch (error) {
      // Provider failures end the run; anything else is a bug and propagates.
      if (!(error instanceof LlmError)) throw error;
      return { error: error.kind === 'aborted' ? 'Exécution annulée' : error.message };
    }
  }

  /** Returns an outcome when the run must pause for validation, otherwise null. */
  private async handleToolTurn(
    state: RunCheckpoint,
    response: LlmResponse,
    signal?: AbortSignal,
  ): Promise<RunOutcome | null> {
    const { ports } = this.deps;
    if (response.text) {
      await ports.events.emit({ type: 'assistant_message', text: response.text, final: false });
    }
    if (response.stopReason === 'max_tokens') {
      // A truncated tool input must never run: report it and let the model retry.
      state.messages.push({ role: 'tool_results', results: truncatedResults(response.toolCalls) });
      await ports.runs.checkpoint(state);
      return null;
    }

    const { results, awaiting } = await this.tools.processTurn(state, response.toolCalls, signal);
    if (awaiting.length > 0) {
      state.pending = { completedResults: results, approvals: awaiting };
      return this.conclude({ status: 'awaiting_approval', approvals: awaiting, checkpoint: state });
    }
    state.messages.push({ role: 'tool_results', results });
    await ports.runs.checkpoint(state);
    return null;
  }

  private async fail(state: RunCheckpoint, error: string): Promise<RunOutcome> {
    return this.conclude({ status: 'failed', error, checkpoint: state });
  }

  /** Records the outcome, then announces it: listeners must find it saved. */
  private async conclude(outcome: RunOutcome): Promise<RunOutcome> {
    const { ports } = this.deps;
    await ports.runs.finish(outcome);
    await ports.events.emit(
      outcome.status === 'failed'
        ? { type: 'run_finished', status: 'failed', error: outcome.error }
        : { type: 'run_finished', status: outcome.status },
    );
    return outcome;
  }
}

function truncatedResults(calls: LlmToolCall[]) {
  return calls.map((c) => ({ toolCallId: c.id, content: TRUNCATED_CALL, isError: true }));
}
