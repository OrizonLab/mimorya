/**
 * Tool contract shared by the agent engine and connectors. A tool never decides
 * authorization on its own: the engine's policy enforcement point checks
 * `requiredPermissions` and autonomy before `execute`, and the tool receives a
 * bound authorization view to filter the data it returns.
 */
import type { z } from 'zod';
import type { ToolRisk } from './autonomy';
import type { Actor, DataScope, Decision, ResourceRef, Subject } from './authz/engine';
import type { Permission } from './authz/permissions';

/** LLM providers accept `^[a-zA-Z0-9_-]{1,64}$`; we use `<namespace>_<action>`. */
export const TOOL_NAME_RE = /^[a-z][a-z0-9_]{1,63}$/;

export interface BoundAuthorization {
  scope(permission: Permission): DataScope;
  check(permission: Permission, resource: Omit<ResourceRef, 'tenantId'>): Decision;
}

export interface DeliverableInput {
  type: string;
  title: string;
  content: unknown;
  orgUnitId?: string | null;
}

export interface Deliverable extends DeliverableInput {
  id: string;
  ownerUserId: string;
  runId: string | null;
  status: 'draft' | 'approved' | 'applied' | 'discarded';
}

/** Platform service letting tools record what the agent produces (drafts, reports…). */
export interface DeliverableService {
  create(input: DeliverableInput): Promise<Deliverable>;
  get(id: string): Promise<Deliverable | null>;
  setStatus(id: string, status: Deliverable['status']): Promise<void>;
}

export interface ToolContext {
  tenantId: string;
  subject: Subject;
  actor: Actor;
  runId: string;
  authz: BoundAuthorization;
  deliverables: DeliverableService;
  signal: AbortSignal;
}

export interface Citation {
  type: string;
  id: string;
  label?: string;
}

export interface ToolResult {
  /** JSON-serialisable payload given to the model (as untrusted data). */
  content: unknown;
  /** Short human summary for activity feeds. */
  summary?: string;
  isError?: boolean;
  /** Identifiers of the records the result is built from (grounding, citations). */
  citations?: Citation[];
}

export interface ToolDefinition<Input = unknown> {
  name: string;
  /** Short French label displayed to users (activity feed, approvals). */
  title: string;
  /** Instructions for the model. */
  description: string;
  inputSchema: z.ZodType<Input>;
  /**
   * JSON Schema sent to the model. Derived from `inputSchema` when absent;
   * provided explicitly for tools whose schema comes from elsewhere (MCP).
   */
  inputJsonSchema?: Record<string, unknown>;
  risk: ToolRisk;
  /** All of these must be held (in some scope) before the tool may run. */
  requiredPermissions: readonly Permission[];
  requiresApproval?: boolean;
  /**
   * Identifiers matching this pattern in an answer must come from this tool's
   * citations (grounding check), e.g. /\bINT-\d{4}\b/g.
   */
  citationPattern?: RegExp;
  /** Human readable description of a pending action, shown on approval requests. */
  describeAction?: (input: Input) => string;
  execute(input: Input, ctx: ToolContext): Promise<ToolResult>;
}

/** Helper preserving the input type inference of a tool definition. */
export function defineTool<Input>(tool: ToolDefinition<Input>): ToolDefinition<Input> {
  if (!TOOL_NAME_RE.test(tool.name)) {
    throw new Error(`Invalid tool name "${tool.name}" (expected ${TOOL_NAME_RE})`);
  }
  return tool;
}

// Tools are stored heterogeneously in registries; the input type is checked
// at runtime through `inputSchema` before `execute` is called.
// eslint-disable-next-line @typescript-eslint/no-explicit-any
export type AnyToolDefinition = ToolDefinition<any>;
