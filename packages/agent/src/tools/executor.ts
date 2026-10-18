/**
 * Policy enforcement point (PEP) for tool calls. The model's request is
 * evaluated against the registry, the input schema, the subject's
 * permissions (with the agent ceiling) and the autonomy policy — in that
 * order — before anything runs.
 */
import {
  decideExecution,
  type Actor,
  type AnyToolDefinition,
  type AuthorizationEngine,
  type AutonomyPolicy,
  type DeliverableService,
  type Subject,
  type ToolContext,
  type ToolResult,
} from '@mimorya/core';
import type { LlmToolCall } from '../llm/types';
import type { ToolRegistry } from './registry';

export type ToolCallEvaluation =
  | { kind: 'allow'; tool: AnyToolDefinition; input: unknown }
  | { kind: 'require_approval'; tool: AnyToolDefinition; input: unknown; reason: string }
  | { kind: 'deny'; tool?: AnyToolDefinition; reason: string };

export interface ToolExecutorDeps {
  engine: AuthorizationEngine;
  registry: ToolRegistry;
  subject: Subject;
  actor: Actor;
  autonomy: AutonomyPolicy;
  runId: string;
  deliverables: DeliverableService;
  timeoutMs?: number;
}

export class ToolExecutor {
  constructor(private readonly deps: ToolExecutorDeps) {}

  evaluate(call: LlmToolCall): ToolCallEvaluation {
    const tool = this.deps.registry.get(call.name);
    if (!tool) return { kind: 'deny', reason: `Outil « ${call.name} » inconnu ou désactivé` };

    const missing = this.missingPermission(tool);
    if (missing) return { kind: 'deny', tool, reason: `Permission ${missing} requise` };

    const parsed = tool.inputSchema.safeParse(call.input);
    if (!parsed.success) {
      const issues = parsed.error.issues
        .slice(0, 5)
        .map((i) => `${i.path.join('.') || '(racine)'} : ${i.message}`)
        .join(' ; ');
      return { kind: 'deny', tool, reason: `Paramètres invalides — ${issues}` };
    }

    const decision = decideExecution(this.deps.autonomy, tool);
    if (decision.kind === 'deny') return { kind: 'deny', tool, reason: decision.reason };
    if (decision.kind === 'require_approval') {
      return { kind: 'require_approval', tool, input: parsed.data, reason: decision.reason };
    }
    return { kind: 'allow', tool, input: parsed.data };
  }

  /**
   * Executes a tool whose call has been allowed (or approved by a human).
   * Permissions are checked again: they may have changed while the call was
   * waiting for validation.
   */
  async execute(
    tool: AnyToolDefinition,
    input: unknown,
    signal?: AbortSignal,
  ): Promise<{ result: ToolResult; durationMs: number }> {
    const started = Date.now();
    const missing = this.missingPermission(tool);
    if (missing) {
      return {
        result: {
          content: { erreur: `Permission ${missing} requise` },
          isError: true,
          summary: 'Refusé',
        },
        durationMs: 0,
      };
    }

    const timeout = AbortSignal.timeout(this.deps.timeoutMs ?? 30_000);
    const combined = signal ? AbortSignal.any([signal, timeout]) : timeout;
    const { engine, subject, actor } = this.deps;
    const ctx: ToolContext = {
      tenantId: subject.tenantId,
      subject,
      actor,
      runId: this.deps.runId,
      deliverables: this.deps.deliverables,
      signal: combined,
      authz: {
        scope: (permission) => engine.dataScope(subject, permission, actor),
        check: (permission, resource) =>
          engine.check(subject, permission, { ...resource, tenantId: subject.tenantId }, actor),
      },
    };

    try {
      const result = await Promise.race([
        tool.execute(input, ctx),
        new Promise<never>((_, reject) => {
          combined.addEventListener('abort', () => reject(new Error('Délai d’exécution dépassé')), {
            once: true,
          });
        }),
      ]);
      return { result, durationMs: Date.now() - started };
    } catch (error) {
      const message = error instanceof Error ? error.message : 'Erreur inconnue';
      return {
        result: { content: { erreur: message }, isError: true, summary: `Échec : ${message}` },
        durationMs: Date.now() - started,
      };
    }
  }

  private missingPermission(tool: AnyToolDefinition) {
    const { engine, subject, actor } = this.deps;
    return tool.requiredPermissions.find((p) => !engine.has(subject, p, actor));
  }
}
