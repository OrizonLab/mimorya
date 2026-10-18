/**
 * Agent run loop: call the model, enforce policies on every tool call,
 * pause for human validation, verify the final answer and correct it within
 * bounds. Every step is checkpointed so that a run can be resumed.
 */
import type {
  Actor,
  AnyToolDefinition,
  AuthorizationEngine,
  AutonomyPolicy,
  Subject,
  ToolResult,
} from '@mimorya/core';
import type { ModelRouter } from '../llm/factory';
import type { PriceTable } from '../llm/pricing';
import {
  LlmError,
  type Effort,
  type LlmMessage,
  type LlmProvider,
  type LlmRequest,
  type LlmResponse,
  type LlmToolCall,
  type LlmToolResult,
} from '../llm/types';
import { ToolExecutor } from '../tools/executor';
import { toToolSpec, type ToolRegistry } from '../tools/registry';
import { truncate, wrapUntrusted } from '../tools/untrusted';
import { checkBudget, type BudgetLimits } from './budget';
import type { EnginePorts } from './ports';
import { ZERO_USAGE, type PendingApproval, type RunCheckpoint } from './state';
import {
  checkGrounding,
  correctionMessage,
  judgeAnswer,
  type VerificationReport,
} from './verification';

export interface RuntimeDeps {
  router: ModelRouter;
  engine: AuthorizationEngine;
  registry: ToolRegistry;
  ports: EnginePorts;
  prices: PriceTable;
  /** Retries for retryable provider errors, on top of the SDK's own retries. */
  maxProviderRetries?: number;
  retryDelayMs?: number;
}

export interface RunConfig {
  runId: string;
  subject: Subject;
  actor: Actor;
  objective: string;
  autonomy: AutonomyPolicy;
  budget: BudgetLimits;
  successCriteria: string[];
  verification: { llmJudge: boolean; maxCorrectionAttempts: number };
  effort?: Effort;
}

export type ApprovalDecision = { approved: boolean; comment?: string; decidedBy?: string };

export type RunOutcome =
  | {
      status: 'completed';
      answer: string;
      verification: VerificationReport;
      checkpoint: RunCheckpoint;
    }
  | { status: 'awaiting_approval'; approvals: PendingApproval[]; checkpoint: RunCheckpoint }
  | { status: 'failed'; error: string; checkpoint: RunCheckpoint };

export class AgentRuntime {
  private readonly executor: ToolExecutor;

  constructor(
    private readonly deps: RuntimeDeps,
    private readonly config: RunConfig,
  ) {
    this.executor = new ToolExecutor({
      engine: deps.engine,
      registry: deps.registry,
      subject: config.subject,
      actor: config.actor,
      autonomy: config.autonomy,
      runId: config.runId,
      deliverables: deps.ports.deliverables,
    });
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
      if (!decision.approved || !tool) {
        const reason = !tool
          ? 'Outil désactivé depuis la demande de validation'
          : `Action refusée par l’utilisateur${decision.comment ? ` : ${decision.comment}` : ''}`;
        results.push({
          toolCallId: approval.toolCallId,
          content: JSON.stringify({ erreur: reason }),
          isError: true,
        });
        await this.deps.ports.toolLog.record({
          toolCallId: approval.toolCallId,
          toolName: approval.toolName,
          input: approval.input,
          decision: 'require_approval',
          status: 'rejected',
          decisionReason: reason,
          approvalId: approval.approvalId,
        });
        continue;
      }
      results.push(
        await this.runTool(
          state,
          tool,
          approval.toolCallId,
          approval.input,
          signal,
          approval.approvalId,
        ),
      );
    }

    delete state.pending;
    state.messages.push({ role: 'tool_results', results: this.orderResults(state, results) });
    await this.deps.ports.runs.checkpoint(state);
    return this.loop(state, signal);
  }

  private async loop(state: RunCheckpoint, signal?: AbortSignal): Promise<RunOutcome> {
    const { ports, registry, engine } = this.deps;
    const startedAt = Date.now() - state.elapsedMs;
    const provider = this.deps.router.for('agent');

    for (;;) {
      state.elapsedMs = Date.now() - startedAt;
      if (signal?.aborted) return this.fail(state, 'Exécution annulée');
      const budget = checkBudget(this.config.budget, state.usage, state.elapsedMs);
      if (!budget.ok) return this.fail(state, `Arrêt : ${budget.reason}`);

      const tools = registry.visibleTo(
        engine,
        this.config.subject,
        this.config.actor,
        this.config.autonomy,
      );
      let response: LlmResponse;
      try {
        response = await this.complete(provider, {
          system: state.system,
          messages: state.messages,
          tools: tools.map(toToolSpec),
          ...(this.config.effort ? { effort: this.config.effort } : {}),
          ...(signal ? { signal } : {}),
          onTextDelta: (text) => void ports.events.emit({ type: 'assistant_delta', text }),
        });
      } catch (error) {
        // Provider failures end the run; anything else is a bug and propagates.
        if (!(error instanceof LlmError)) throw error;
        return this.fail(state, error.kind === 'aborted' ? 'Exécution annulée' : error.message);
      }
      state.usage.steps += 1;
      await this.recordUsage(state, response);

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
        if (response.text) {
          await ports.events.emit({ type: 'assistant_message', text: response.text, final: false });
        }
        if (response.stopReason === 'max_tokens') {
          // A truncated tool input must never run: report it and let the model retry.
          state.messages.push({
            role: 'tool_results',
            results: response.toolCalls.map((c) => ({
              toolCallId: c.id,
              content: JSON.stringify({
                erreur: 'Appel tronqué (limite de longueur atteinte), non exécuté.',
              }),
              isError: true,
            })),
          });
          await ports.runs.checkpoint(state);
          continue;
        }
        const paused = await this.handleToolCalls(state, response.toolCalls, signal);
        if (paused) return paused;
        continue;
      }

      // Final answer candidate.
      const answer = response.text.trim();
      const report = await this.verify(state, answer, response.stopReason === 'max_tokens');
      await ports.events.emit({
        type: 'verification',
        attempt: state.correctionAttempts + 1,
        passed: report.passed,
        issues: report.issues,
      });
      if (
        !report.passed &&
        state.correctionAttempts < this.config.verification.maxCorrectionAttempts
      ) {
        state.correctionAttempts += 1;
        state.messages.push({ role: 'user', content: correctionMessage(report.issues) });
        await ports.runs.checkpoint(state);
        continue;
      }

      const finalAnswer = report.passed
        ? answer
        : `${answer}\n\n---\n⚠️ **Points non vérifiés par le contrôle qualité :**\n${report.issues.map((i) => `- ${i}`).join('\n')}`;
      state.elapsedMs = Date.now() - startedAt;
      await ports.runs.checkpoint(state);
      await ports.events.emit({ type: 'assistant_message', text: finalAnswer, final: true });
      await ports.events.emit({ type: 'run_finished', status: 'completed' });
      return { status: 'completed', answer: finalAnswer, verification: report, checkpoint: state };
    }
  }

  /** Returns an outcome when the run must pause for validation, otherwise null. */
  private async handleToolCalls(
    state: RunCheckpoint,
    calls: LlmToolCall[],
    signal?: AbortSignal,
  ): Promise<RunOutcome | null> {
    const { ports } = this.deps;
    const results: LlmToolResult[] = [];
    const awaiting: PendingApproval[] = [];

    for (const call of calls) {
      const evaluation = this.executor.evaluate(call);
      const title = evaluation.tool?.title ?? call.name;
      await ports.events.emit({
        type: 'tool_call',
        toolCallId: call.id,
        toolName: call.name,
        title,
        decision: evaluation.kind,
        ...(evaluation.kind !== 'allow' ? { reason: evaluation.reason } : {}),
      });

      if (evaluation.kind === 'deny') {
        results.push({
          toolCallId: call.id,
          content: JSON.stringify({
            erreur: `Refusé par la plateforme : ${evaluation.reason}`,
            consigne:
              'Ne tente pas de contourner ce refus ; adapte ta réponse ou explique la limite.',
          }),
          isError: true,
        });
        await ports.toolLog.record({
          toolCallId: call.id,
          toolName: call.name,
          input: call.input,
          decision: 'deny',
          decisionReason: evaluation.reason,
          status: 'denied',
        });
        await ports.audit.record({
          action: 'agent.tool.denied',
          outcome: 'denied',
          resourceType: 'tool',
          resourceId: call.name,
          reason: evaluation.reason,
        });
        continue;
      }

      if (evaluation.kind === 'require_approval') {
        const tool = evaluation.tool;
        const actionSummary =
          tool.describeAction?.(evaluation.input) ?? `${tool.title} (${call.name})`;
        const { id } = await ports.approvals.create({
          toolName: tool.name,
          toolCallId: call.id,
          actionSummary,
          payload: evaluation.input,
          risk: tool.risk,
          reason: evaluation.reason,
        });
        awaiting.push({
          approvalId: id,
          toolCallId: call.id,
          toolName: tool.name,
          input: evaluation.input,
        });
        await ports.toolLog.record({
          toolCallId: call.id,
          toolName: tool.name,
          input: evaluation.input,
          decision: 'require_approval',
          decisionReason: evaluation.reason,
          status: 'awaiting_approval',
          approvalId: id,
        });
        await ports.audit.record({
          action: 'agent.approval.requested',
          outcome: 'success',
          resourceType: 'approval',
          resourceId: id,
          reason: evaluation.reason,
          metadata: { toolName: tool.name, risk: tool.risk },
        });
        await ports.events.emit({
          type: 'approval_requested',
          approvalId: id,
          toolName: tool.name,
          actionSummary,
          risk: tool.risk,
        });
        continue;
      }

      results.push(await this.runTool(state, evaluation.tool, call.id, evaluation.input, signal));
    }

    if (awaiting.length > 0) {
      state.pending = { completedResults: results, approvals: awaiting };
      await ports.runs.checkpoint(state);
      await ports.events.emit({ type: 'run_finished', status: 'awaiting_approval' });
      return { status: 'awaiting_approval', approvals: awaiting, checkpoint: state };
    }
    state.messages.push({ role: 'tool_results', results });
    await ports.runs.checkpoint(state);
    return null;
  }

  private async runTool(
    state: RunCheckpoint,
    tool: AnyToolDefinition,
    toolCallId: string,
    input: unknown,
    signal?: AbortSignal,
    approvalId?: string,
  ): Promise<LlmToolResult> {
    const { ports } = this.deps;
    const { result, durationMs } = await this.executor.execute(tool, input, signal);
    state.usage.toolCalls += 1;
    if (!result.isError && result.citations) state.evidence.push(...result.citations);
    const summary = result.summary ?? (result.isError ? 'Échec' : 'Terminé');

    await ports.toolLog.record({
      toolCallId,
      toolName: tool.name,
      input,
      decision: approvalId ? 'require_approval' : 'allow',
      status: result.isError ? 'failed' : 'succeeded',
      outputSummary: summary,
      ...(result.isError ? { error: errorText(result) } : {}),
      durationMs,
      ...(approvalId ? { approvalId } : {}),
    });
    await ports.audit.record({
      action: 'agent.tool.execute',
      outcome: result.isError ? 'failure' : 'success',
      resourceType: 'tool',
      resourceId: tool.name,
      metadata: { risk: tool.risk, durationMs, ...(approvalId ? { approvalId } : {}) },
    });
    await ports.events.emit({
      type: 'tool_result',
      toolCallId,
      toolName: tool.name,
      summary,
      isError: !!result.isError,
      durationMs,
    });
    return {
      toolCallId,
      content: truncate(wrapUntrusted(tool.name, result.content)),
      ...(result.isError ? { isError: true } : {}),
    };
  }

  private async verify(
    state: RunCheckpoint,
    answer: string,
    truncated: boolean,
  ): Promise<VerificationReport> {
    const issues: string[] = [];
    if (!answer) issues.push('La réponse est vide.');
    if (truncated) issues.push('La réponse a été tronquée : fournis une version plus concise.');
    const patterns = this.deps.registry
      .all()
      .map((t) => t.citationPattern)
      .filter((p): p is RegExp => !!p);
    issues.push(...checkGrounding(answer, patterns, state.evidence));

    let judged = false;
    const { llmJudge } = this.config.verification;
    if (issues.length === 0 && llmJudge && this.config.successCriteria.length > 0) {
      try {
        const { verdict, response } = await judgeAnswer(this.deps.router.for('verification'), {
          objective: this.config.objective,
          criteria: this.config.successCriteria,
          answer,
          evidence: state.evidence,
        });
        await this.recordUsage(state, response);
        if (verdict) {
          judged = true;
          if (!verdict.pass)
            issues.push(...(verdict.issues.length ? verdict.issues : ['Critères non satisfaits.']));
        }
      } catch {
        // A failing reviewer must not block the answer; the report says it was not judged.
      }
    }
    return { passed: issues.length === 0, issues, judged };
  }

  private async complete(provider: LlmProvider, request: LlmRequest): Promise<LlmResponse> {
    const maxRetries = this.deps.maxProviderRetries ?? 2;
    for (let attempt = 0; ; attempt++) {
      try {
        return await provider.complete(request);
      } catch (error) {
        if (!(error instanceof LlmError) || !error.retryable || attempt >= maxRetries) throw error;
        const delay = (this.deps.retryDelayMs ?? 1000) * 2 ** attempt;
        await new Promise((resolve) => setTimeout(resolve, delay));
      }
    }
  }

  private async recordUsage(state: RunCheckpoint, response: LlmResponse): Promise<void> {
    const cost = this.deps.prices.costMicros(response.model, response.usage);
    state.usage.inputTokens +=
      response.usage.inputTokens + response.usage.cacheReadTokens + response.usage.cacheWriteTokens;
    state.usage.outputTokens += response.usage.outputTokens;
    state.usage.costMicros += cost ?? 0;
    await this.deps.ports.usage.record({
      kind: 'llm',
      provider: response.provider,
      model: response.model,
      inputTokens:
        response.usage.inputTokens +
        response.usage.cacheReadTokens +
        response.usage.cacheWriteTokens,
      outputTokens: response.usage.outputTokens,
      costMicros: cost ?? 0,
    });
    await this.deps.ports.events.emit({
      type: 'usage',
      inputTokens: response.usage.inputTokens,
      outputTokens: response.usage.outputTokens,
      costMicros: cost,
    });
  }

  /** Tool results are returned in the order the model requested the calls. */
  private orderResults(state: RunCheckpoint, results: LlmToolResult[]): LlmToolResult[] {
    const lastAssistant = [...state.messages].reverse().find((m) => m.role === 'assistant');
    if (!lastAssistant || lastAssistant.role !== 'assistant') return results;
    const order = new Map(lastAssistant.toolCalls.map((c, i) => [c.id, i]));
    return [...results].sort(
      (a, b) => (order.get(a.toolCallId) ?? 0) - (order.get(b.toolCallId) ?? 0),
    );
  }

  private async fail(state: RunCheckpoint, error: string): Promise<RunOutcome> {
    await this.deps.ports.runs.checkpoint(state);
    await this.deps.ports.events.emit({ type: 'run_finished', status: 'failed', error });
    return { status: 'failed', error, checkpoint: state };
  }
}

function errorText(result: ToolResult): string {
  const content = result.content as { erreur?: unknown } | undefined;
  return typeof content?.erreur === 'string' ? content.erreur : (result.summary ?? 'Erreur');
}
