import {
  decideExecution,
  type Actor,
  type AnyToolDefinition,
  type AuthorizationEngine,
  type AutonomyPolicy,
  type Permission,
  type Subject,
  type ToolRisk,
} from '@mimorya/core';
import { z } from 'zod';
import type { LlmToolSpec } from '../llm/types';

/** Administrator overrides for a tool (from connector configuration). */
export interface ToolOverride {
  enabled?: boolean;
  permissions?: Permission[];
  risk?: ToolRisk;
  requiresApproval?: boolean;
}

/**
 * The tools available to one tenant, after administrator overrides. Tools
 * are filtered per subject before being shown to a model.
 */
export class ToolRegistry {
  private readonly tools = new Map<string, AnyToolDefinition>();

  constructor(
    tools: readonly AnyToolDefinition[] = [],
    overrides: Record<string, ToolOverride> = {},
  ) {
    for (const tool of tools) this.register(tool, overrides[tool.name]);
  }

  register(tool: AnyToolDefinition, override?: ToolOverride): void {
    if (this.tools.has(tool.name)) throw new Error(`Duplicate tool ${tool.name}`);
    if (override?.enabled === false) return;
    this.tools.set(tool.name, {
      ...tool,
      ...(override?.permissions ? { requiredPermissions: override.permissions } : {}),
      ...(override?.risk ? { risk: override.risk } : {}),
      ...(override?.requiresApproval !== undefined
        ? { requiresApproval: override.requiresApproval }
        : {}),
    });
  }

  get(name: string): AnyToolDefinition | undefined {
    return this.tools.get(name);
  }

  all(): AnyToolDefinition[] {
    return [...this.tools.values()];
  }

  /**
   * Tools the agent may offer to the model for this subject: required
   * permissions held (agent ceiling applied) and not forbidden by autonomy.
   */
  visibleTo(engine: AuthorizationEngine, subject: Subject, actor: Actor, autonomy: AutonomyPolicy) {
    return this.all().filter(
      (tool) =>
        tool.requiredPermissions.every((p) => engine.has(subject, p, actor)) &&
        decideExecution(autonomy, tool).kind !== 'deny',
    );
  }
}

export function toToolSpec(tool: AnyToolDefinition): LlmToolSpec {
  const schema =
    tool.inputJsonSchema ??
    (z.toJSONSchema(tool.inputSchema, { io: 'input' }) as Record<string, unknown>);
  // `$schema` is noise for providers and some reject it.
  const { $schema: _ignored, ...inputSchema } = schema;
  return { name: tool.name, description: tool.description, inputSchema };
}
