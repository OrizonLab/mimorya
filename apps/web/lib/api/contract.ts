// Generated from the API's Rust types (crates/api/src/contract, ADR-036): do not edit.
// Regenerate with `pnpm contract`.

export type AccountStatus = "active" | "invited" | "disabled";

/**
 * An action of the agent waiting for (or given) a human decision.
 */
export type Approval = { id: string, runId: string, toolName: string, actionSummary: string,
/**
 * The validated input, executed as is once approved.
 */
payload: unknown, risk: ToolRisk, reason: string, status: string, decisionComment: string | null, createdAt: string, decidedAt: string | null, };

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

export type ApprovalRequestedEvent = { approvalId: string, toolName: string, actionSummary: string, risk: ToolRisk, };

export type ApprovalResolvedEvent = { approvalId: string, approved: boolean, };

/**
 * A role held by an account.
 */
export type Assignment = { id: string, roleId: string, roleName: string, scope: AssignmentScope,
/**
 * Role and scope, readable.
 */
label: string, expiresAt: string | null, };

/**
 * Where a role applies.
 */
export type AssignmentScope = { "type": "tenant" } | { "type": "org_unit", orgUnitId: string, includeDescendants: boolean, } | { "type": "self" };

export type AssistantDelta = { text: string, };

export type AssistantMessage = { text: string,
/**
 * The answer, rather than a note written between tool calls.
 */
final: boolean, };

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
 * A resource created (`201`): its identifier.
 */
export type Created = { id: string, };

/**
 * `GET /admin/users`.
 */
export type Directory = { users: Array<DirectoryUser>, canManage: DirectoryRights,
/**
 * Invitations and reset links go by e-mail.
 */
emailEnabled: boolean, };

/**
 * Account and role management, each requiring its permission company-wide.
 */
export type DirectoryRights = { users: boolean, roles: boolean, };

export type DirectoryUser = { id: string, displayName: string, jobTitle: string | null, status: AccountStatus,
/**
 * Its primary unit, as a path from the root.
 */
orgUnit: string | null,
/**
 * To account or role managers only.
 */
email?: string, lastLoginAt?: string | null, orgUnitId?: string | null, clearance?: Sensitivity,
/**
 * To account managers only: read by access policies as
 * `subject.attributes.*` (ADR-023).
 */
attributes?: Record<string, unknown>,
/**
 * To account managers only: when the pending invitation link expires.
 */
invitationExpiresAt?: string | null,
/**
 * To role managers only.
 */
grants?: Array<string>, assignments?: Array<Assignment>, };

/**
 * A change done, with nothing else to return.
 */
export type Done = { ok: boolean, };

/**
 * Where a permission is checked so far.
 */
export type Enforcement = "not_yet" | "holder" | "per_resource";

/**
 * `GET /health`: answered once the database answers.
 */
export type Health = {
/**
 * Always `ok`.
 */
status: string,
/**
 * A platform model provider is configured (companies may have their own).
 */
llm: boolean, };

/**
 * `POST /auth/invitation`: who the invitation is for.
 */
export type InvitationDetails = { email: string, displayName: string, company: string, expiresAt: string, };

/**
 * `POST /admin/users/{id}/invitation`: a new link; the former one stops working.
 */
export type InvitationIssued = { invitation: IssuedInvitation, };

/**
 * An invitation link just issued: e-mailed, or to transmit.
 */
export type IssuedInvitation = { expiresAt: string, emailed: boolean,
/**
 * Shown once, when no e-mail went out.
 */
link: string | null,
/**
 * Why no e-mail went out.
 */
problem: string | null, };

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

export type MemoryItem = { id: string, scope: MemoryScope,
/**
 * Name of the unit sharing it, for a unit's memory.
 */
orgUnit: string | null, kind: MemoryKind, title: string, content: string, status: MemoryStatus, source: MemorySource,
/**
 * How many times the assistant recalled it.
 */
useCount: number, lastUsedAt: string | null, updatedAt: string,
/**
 * The user may delete it (and validate it, when proposed).
 */
canManage: boolean, };

export type MemoryKind = "preference" | "fact" | "procedure" | "decision" | "lesson" | "episode";

/**
 * `GET /memory`.
 */
export type MemoryList = { items: Array<MemoryItem>, };

/**
 * The space a memory belongs to.
 */
export type MemoryScope = "user" | "org_unit" | "tenant" | "experience";

/**
 * Where a memory comes from: written by hand, or by a run of the assistant.
 */
export type MemorySource = { type: string, id?: string, label?: string, };

export type MemoryStatus = "active" | "proposed" | "archived";

export type Message = { id: string, role: MessageRole, content: string, runId: string | null, metadata: MessageMetadata, createdAt: string, };

/**
 * `POST /conversations/{id}/messages`: the run started for the message.
 */
export type MessageAccepted = { runId: string, messageId: string, };

/**
 * What the platform records with a message (`messages.metadata`): written
 * with this type (`services::runs`), read back with it, a key off the
 * contract being an error.
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

/**
 * An account as the organisation screen names it.
 */
export type OrgPerson = { id: string, displayName: string, jobTitle: string | null, };

export type OrgUnitRef = { id: string,
/**
 * Its place in the organisation, from the root.
 */
path: string, };

export type OrgUnitType = "company" | "site" | "direction" | "department" | "service" | "team";

export type OrgUnitView = { id: string, parentId: string | null, name: string, type: OrgUnitType,
/**
 * Organisational lead: informational, it grants no access by itself.
 */
leadUserId: string | null,
/**
 * Accounts whose primary unit it is.
 */
members: Array<string>, };

/**
 * `GET /admin/organization`.
 */
export type Organization = { units: Array<OrgUnitView>, users: Array<OrgPerson>,
/**
 * Changing the tree requires `org:manage` on the whole company.
 */
canManage: boolean, };

export type PermissionView = { permission: string, label: string,
/**
 * `false`: the agent can never exercise it.
 */
delegable: boolean, enforcement: Enforcement, };

/**
 * `POST /auth/password-reset/link`: the account whose password is reset.
 */
export type ResetDetails = { email: string, expiresAt: string, };

/**
 * `POST /admin/users/{id}/password-link`: the administrator never sees the link.
 */
export type ResetLinkSent = { expiresAt: string, };

export type RoleView = { id: string, key: string, name: string, description: string,
/**
 * Provided by the platform: neither changed nor deleted.
 */
isSystem: boolean, permissions: Array<string>,
/**
 * Assignments of the role, expired ones included.
 */
assignments: number, heldByYou: boolean, };

/**
 * `GET /admin/roles`.
 */
export type Roles = { roles: Array<RoleView>, catalog: Array<PermissionView>, };

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
 * The run stopped: ended, or paused for a validation (`awaiting_approval`).
 */
export type RunFinished = { status: RunStatus, error?: string, };

/**
 * `GET /runs`.
 */
export type RunList = { runs: Array<RunListItem>, };

/**
 * A run in the list of the user's runs.
 */
export type RunListItem = { id: string, conversationId: string | null, objective: string, status: RunStatus, usage: RunUsage, error: string | null, createdAt: string, finishedAt: string | null, };

/**
 * A run of a conversation, listed with its messages.
 */
export type RunRef = { id: string, status: RunStatus, createdAt: string, };

export type RunStarted = { objective: string, };

export type RunStatus = "queued" | "running" | "awaiting_approval" | "completed" | "failed" | "cancelled";

/**
 * One event of the stream, by name.
 */
export type RunStreamEvent = { "type": "run_started", "data": RunStarted } | { "type": "assistant_delta", "data": AssistantDelta } | { "type": "assistant_message", "data": AssistantMessage } | { "type": "tool_call", "data": ToolCallEvent } | { "type": "tool_result", "data": ToolResultEvent } | { "type": "approval_requested", "data": ApprovalRequestedEvent } | { "type": "approval_resolved", "data": ApprovalResolvedEvent } | { "type": "verification", "data": VerificationEvent } | { "type": "usage", "data": UsageEvent } | { "type": "run_finished", "data": RunFinished };

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
 * Highest sensitivity of the data an account may read.
 */
export type Sensitivity = "public" | "internal" | "confidential" | "restricted";

/**
 * `GET /admin/skills`.
 */
export type SkillList = { skills: Array<SkillView>, };

export type SkillStatus = "draft" | "active" | "retired";

export type SkillView = { id: string, name: string, description: string, status: SkillStatus, activeVersion: number | null, updatedAt: string, };

/**
 * `POST /admin/users/{id}/password`: shown once, replaced at next sign-in.
 */
export type TemporaryPassword = { temporaryPassword: string, };

export type ToolCallEvent = { toolCallId: string, toolName: string, title: string, decision: ToolDecision, reason?: string, };

/**
 * The policy's decision on a tool call.
 */
export type ToolDecision = "allow" | "require_approval" | "deny";

/**
 * A tool call of a run, with the decision taken on it.
 */
export type ToolInvocation = { id: string, toolName: string, decision: ToolDecision, decisionReason: string | null, status: string, outputSummary: string | null, durationMs: number | null, createdAt: string, };

export type ToolResultEvent = { toolCallId: string, toolName: string, summary: string, isError: boolean, durationMs: number, };

/**
 * Risk class of a tool, from harmless to irreversible.
 */
export type ToolRisk = "read" | "draft" | "write" | "external" | "irreversible";

/**
 * Tokens (and cost) of one model call.
 */
export type UsageEvent = { inputTokens: number, outputTokens: number,
/**
 * `null` when the model is not priced.
 */
costMicros: number | null, };

/**
 * `POST /admin/users`.
 */
export type UserCreated = { id: string, invitation: IssuedInvitation, };

/**
 * Checks of a run's final answer.
 */
export type Verification = { passed: boolean, issues: Array<string>,
/**
 * True when a model review ran and gave a usable verdict.
 */
judged: boolean, };

/**
 * Checks of a candidate answer; a failed attempt may be followed by another.
 */
export type VerificationEvent = { attempt: number, passed: boolean, issues: Array<string>, };
