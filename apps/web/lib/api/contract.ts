// Generated from the API's Rust types (crates/api/src/contract, ADR-036): do not edit.
// Regenerate with `pnpm contract`.

/**
 * An action of the agent waiting for (or given) a human decision.
 */
export type Approval = { id: string, runId: string, toolName: string, actionSummary: string,
/**
 * The validated input, executed as is once approved.
 */
payload: unknown, risk: string, reason: string, status: string, decisionComment: string | null, createdAt: string, decidedAt: string | null, };

/**
 * `POST /approvals/{id}/decision`.
 */
export type ApprovalDecided = { approval: Approval,
/**
 * True when this decision was the run's last one awaited: the run resumes.
 */
resumed: boolean, };

/**
 * `GET /approvals`.
 */
export type ApprovalList = { approvals: Array<Approval>, };

export type AssistantStatus = { name: string,
/**
 * Whether the company's runs have a model provider they can use.
 */
available: boolean, };

/**
 * `GET /auth/options`: what the sign-in page may offer.
 */
export type AuthOptions = {
/**
 * An e-mail relay is configured: "forgotten password" can send a link.
 */
passwordReset: boolean, };

/**
 * A source the agent's answer relies on: an identifier returned by a tool.
 */
export type Citation = { type: string, id: string, label?: string, };

/**
 * `POST /conversations`.
 */
export type ConversationCreated = { conversation: ConversationSummary, };

/**
 * `GET /conversations/{id}`.
 */
export type ConversationDetail = { conversation: ConversationSummary, messages: Array<Message>, runs: Array<RunRef>, };

/**
 * `GET /conversations`.
 */
export type ConversationList = { conversations: Array<ConversationSummary>, };

export type ConversationSummary = { id: string, title: string, updatedAt: string, };

/**
 * A change done, with nothing else to return.
 */
export type Done = { ok: boolean, };

/**
 * `GET /auth/me`.
 */
export type Me = { user: MeUser, tenant: MeTenant,
/**
 * Roles held, readable: "Responsable d’équipe — Service technique et sous-unités".
 */
grants: Array<string>,
/**
 * Permissions of the catalog held somewhere.
 */
permissions: Array<string>, assistant: AssistantStatus, };

export type MeTenant = { id: string, name: string,
/**
 * The demonstration company (fictitious data).
 */
demo: boolean, };

export type MeUser = { id: string, email: string, displayName: string, jobTitle: string | null, orgUnit: OrgUnitRef | null,
/**
 * Temporary password: nothing else is reachable until it is replaced.
 */
mustChangePassword: boolean, };

export type Message = { id: string, role: MessageRole, content: string, runId: string | null, metadata: MessageMetadata, createdAt: string, };

/**
 * `POST /conversations/{id}/messages`: the run started for the message.
 */
export type MessageAccepted = { runId: string, messageId: string, };

/**
 * What the platform records with an assistant's message.
 */
export type MessageMetadata = {
/**
 * How the run that wrote the message ended.
 */
status?: RunStatus,
/**
 * The first sources collected by the run.
 */
evidence?: Array<Citation>, verification?: Verification, };

export type MessageRole = "user" | "assistant";

export type OrgUnitRef = { id: string,
/**
 * Its place in the organisation, from the root.
 */
path: string, };

/**
 * The limits a run stops at (`agent_runs.budget`).
 */
export type RunBudget = { maxSteps: number, maxInputTokens: number, maxOutputTokens: number, maxCostMicros: number, maxDurationMs: number, };

/**
 * `GET /runs/{id}`.
 */
export type RunDetail = { run: RunView, tools: Array<ToolInvocation>, approvals: Array<Approval>, events: Array<RunEventRecord>, };

/**
 * An event of a run's journal, as replayed by its event stream.
 */
export type RunEventRecord = { seq: number, type: string, data: unknown, createdAt: string, };

/**
 * `GET /runs`.
 */
export type RunList = { runs: Array<RunSummary>, };

/**
 * A run of a conversation, listed with its messages.
 */
export type RunRef = { id: string, status: RunStatus, createdAt: string, };

export type RunStatus = "queued" | "running" | "awaiting_approval" | "completed" | "failed" | "cancelled";

/**
 * A run in the list of the user's runs.
 */
export type RunSummary = { id: string, conversationId: string | null, objective: string, status: RunStatus, usage: RunUsage, error: string | null, createdAt: string, finishedAt: string | null, };

/**
 * What a run has consumed so far (`agent_runs.usage`).
 */
export type RunUsage = { steps: number, toolCalls: number, inputTokens: number, outputTokens: number,
/**
 * Estimated cost, in millionths of the currency unit.
 */
costMicros: number, };

export type RunView = { id: string, conversationId: string | null, objective: string, status: RunStatus, autonomyLevel: string, usage: RunUsage, budget: RunBudget, verification: Verification | null, error: string | null, createdAt: string, finishedAt: string | null, };

/**
 * A tool call of a run, with the decision taken on it.
 */
export type ToolInvocation = { id: string, toolName: string, decision: string, decisionReason: string | null, status: string, outputSummary: string | null, durationMs: number | null, createdAt: string, };

/**
 * Checks of a run's final answer.
 */
export type Verification = { passed: boolean, issues: Array<string>,
/**
 * True when a model review ran and gave a usable verdict.
 */
judged: boolean, };
