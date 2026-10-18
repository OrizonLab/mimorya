/**
 * Handling of the tool calls of one assistant turn: each call goes through the
 * policy enforcement point, then is denied, queued for human validation or
 * executed. Every outcome is logged, audited and streamed.
 */
import type { AnyToolDefinition, ToolResult } from '@mimorya/core';
import type { LlmToolCall, LlmToolResult } from '../llm/types';
import type { ToolExecutor } from '../tools/executor';
import { truncate, wrapUntrusted } from '../tools/untrusted';
import type { EnginePorts } from './ports';
import type { PendingApproval, RunCheckpoint } from './state';

export interface TurnToolResults {
  /** Results of the calls that were denied or executed. */
  results: LlmToolResult[];
  /** Calls waiting for a human decision (the run must pause when non-empty). */
  awaiting: PendingApproval[];
}

export class ToolCallProcessor {
  constructor(
    private readonly executor: ToolExecutor,
    private readonly ports: EnginePorts,
  ) {}

  async processTurn(
    state: RunCheckpoint,
    calls: LlmToolCall[],
    signal?: AbortSignal,
  ): Promise<TurnToolResults> {
    const turn: TurnToolResults = { results: [], awaiting: [] };
    for (const call of calls) {
      const evaluation = this.executor.evaluate(call);
      await this.ports.events.emit({
        type: 'tool_call',
        toolCallId: call.id,
        toolName: call.name,
        title: evaluation.tool?.title ?? call.name,
        decision: evaluation.kind,
        ...(evaluation.kind !== 'allow' ? { reason: evaluation.reason } : {}),
      });

      if (evaluation.kind === 'deny') {
        turn.results.push(await this.deny(call, evaluation.reason));
      } else if (evaluation.kind === 'require_approval') {
        turn.awaiting.push(
          await this.requestApproval(call, evaluation.tool, evaluation.input, evaluation.reason),
        );
      } else {
        turn.results.push(
          await this.run(state, evaluation.tool, call.id, evaluation.input, signal),
        );
      }
    }
    return turn;
  }

  /** Executes an allowed (or human-approved) call and turns its result into model input. */
  async run(
    state: RunCheckpoint,
    tool: AnyToolDefinition,
    toolCallId: string,
    input: unknown,
    signal?: AbortSignal,
    approvalId?: string,
  ): Promise<LlmToolResult> {
    const { result, durationMs } = await this.executor.execute(tool, input, signal);
    state.usage.toolCalls += 1;
    if (!result.isError && result.citations) state.evidence.push(...result.citations);
    const summary = result.summary ?? (result.isError ? 'Échec' : 'Terminé');

    await this.ports.toolLog.record({
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
    await this.ports.audit.record({
      action: 'agent.tool.execute',
      outcome: result.isError ? 'failure' : 'success',
      resourceType: 'tool',
      resourceId: tool.name,
      metadata: { risk: tool.risk, durationMs, ...(approvalId ? { approvalId } : {}) },
    });
    await this.ports.events.emit({
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

  /** A call refused by a human (or whose tool disappeared) is reported to the model. */
  async reject(approval: PendingApproval, reason: string): Promise<LlmToolResult> {
    await this.ports.toolLog.record({
      toolCallId: approval.toolCallId,
      toolName: approval.toolName,
      input: approval.input,
      decision: 'require_approval',
      status: 'rejected',
      decisionReason: reason,
      approvalId: approval.approvalId,
    });
    return {
      toolCallId: approval.toolCallId,
      content: JSON.stringify({ erreur: reason }),
      isError: true,
    };
  }

  private async deny(call: LlmToolCall, reason: string): Promise<LlmToolResult> {
    await this.ports.toolLog.record({
      toolCallId: call.id,
      toolName: call.name,
      input: call.input,
      decision: 'deny',
      decisionReason: reason,
      status: 'denied',
    });
    await this.ports.audit.record({
      action: 'agent.tool.denied',
      outcome: 'denied',
      resourceType: 'tool',
      resourceId: call.name,
      reason,
    });
    return {
      toolCallId: call.id,
      content: JSON.stringify({
        erreur: `Refusé par la plateforme : ${reason}`,
        consigne: 'Ne tente pas de contourner ce refus ; adapte ta réponse ou explique la limite.',
      }),
      isError: true,
    };
  }

  private async requestApproval(
    call: LlmToolCall,
    tool: AnyToolDefinition,
    input: unknown,
    reason: string,
  ): Promise<PendingApproval> {
    const actionSummary = tool.describeAction?.(input) ?? `${tool.title} (${call.name})`;
    const { id } = await this.ports.approvals.create({
      toolName: tool.name,
      toolCallId: call.id,
      actionSummary,
      payload: input,
      risk: tool.risk,
      reason,
    });
    await this.ports.toolLog.record({
      toolCallId: call.id,
      toolName: tool.name,
      input,
      decision: 'require_approval',
      decisionReason: reason,
      status: 'awaiting_approval',
      approvalId: id,
    });
    await this.ports.audit.record({
      action: 'agent.approval.requested',
      outcome: 'success',
      resourceType: 'approval',
      resourceId: id,
      reason,
      metadata: { toolName: tool.name, risk: tool.risk },
    });
    await this.ports.events.emit({
      type: 'approval_requested',
      approvalId: id,
      toolName: tool.name,
      actionSummary,
      risk: tool.risk,
    });
    return { approvalId: id, toolCallId: call.id, toolName: tool.name, input };
  }
}

/** Tool results are returned in the order the model requested the calls. */
export function orderByRequest(state: RunCheckpoint, results: LlmToolResult[]): LlmToolResult[] {
  const lastAssistant = [...state.messages].reverse().find((m) => m.role === 'assistant');
  if (!lastAssistant || lastAssistant.role !== 'assistant') return results;
  const order = new Map(lastAssistant.toolCalls.map((c, i) => [c.id, i]));
  return [...results].sort(
    (a, b) => (order.get(a.toolCallId) ?? 0) - (order.get(b.toolCallId) ?? 0),
  );
}

function errorText(result: ToolResult): string {
  const content = result.content as { erreur?: unknown } | undefined;
  return typeof content?.erreur === 'string' ? content.erreur : (result.summary ?? 'Erreur');
}
