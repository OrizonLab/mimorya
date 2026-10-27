// Generated from the API's Rust types (crates/api/src/contract, ADR-036): do not edit.
// Regenerate with `pnpm contract`.

/**
 * Body of `PUT /approval-delegation`: declares, or replaces, the reader's
 * absence.
 */
export type Absence = {
/**
 * The colleague who stands in.
 */
delegateId: string,
/**
 * ISO 8601; absent, from now.
 */
startsAt?: string,
/**
 * ISO 8601, at most 90 days after the start.
 */
endsAt: string, };

/**
 * Answer of `PUT` (body `Absence`) and `DELETE /admin/users/{id}/absence`.
 */
export type AbsenceRecorded = {
/**
 * Whether the collaborator was told by e-mail: not without a relay, nor
 * when sending failed.
 */
notified: boolean, };

export type AccountStatus = "active" | "invited" | "disabled";

/**
 * Body of `PATCH /admin/users/{id}`: only the fields sent change.
 */
export type AccountUpdate = { displayName?: string,
/**
 * `null` or blank clears it.
 */
jobTitle?: string | null,
/**
 * `null`: no primary unit.
 */
orgUnitId?: string | null, clearance?: Sensitivity, status?: StatusChange,
/**
 * Replaces every attribute; `{}` removes them all.
 */
attributes?: Record<string, unknown>, };

/**
 * Who works a run: the agent the user talks to, or one of its sub-agents
 * (ADR-098).
 */
export type AgentProfile = "main" | "research" | "analysis" | "planning" | "quality";

/**
 * An action of the agent waiting for (or given) a human decision.
 */
export type Approval = { id: string, runId: string, toolName: string, actionSummary: string,
/**
 * The validated input, executed as is once approved.
 */
payload: unknown, risk: ToolRisk, reason: string, status: string, decisionComment: string | null, createdAt: string, decidedAt: string | null,
/**
 * After it, nobody may decide it any more.
 */
expiresAt: string | null,
/**
 * Whom the agent acts for.
 */
requestedFor: ApprovalPerson,
/**
 * Who decides it: whom the agent acts for, or the manager named when it
 * was made (ADR-059); `null` once that manager's account is gone.
 */
approver: ApprovalPerson | null, decidedBy: ApprovalPerson | null,
/**
 * Whether the reader may decide it now: approve or refuse it.
 */
canDecide: boolean,
/**
 * Whether the reader may withdraw it now: made for them, a manager
 * decides it (ADR-059). Withdrawn, it is refused.
 */
canWithdraw: boolean, };

/**
 * `POST /approvals/{id}/decision`.
 */
export type ApprovalDecided = { approval: Approval,
/**
 * True when this decision was the run's last one awaited: the run resumes.
 */
resumed: boolean, };

/**
 * `GET /approval-delegation`.
 */
export type ApprovalDelegations = {
/**
 * The reader's absence, declared and not ended: who stands in for them.
 */
own: Delegation | null,
/**
 * The absences the reader covers, under way or to come.
 */
standingInFor: Array<Delegation>,
/**
 * Whom the reader may name, by name: active colleagues holding
 * `approval:decide` on a unit or the company (ADR-073). Empty unless
 * the reader holds it too.
 */
candidates: Array<ApprovalPerson>, };

/**
 * `GET /approvals`.
 */
export type ApprovalList = { approvals: Array<Approval>, };

/**
 * Someone a request names.
 */
export type ApprovalPerson = { id: string, displayName: string, };

export type ApprovalRequestedEvent = { approvalId: string, toolName: string, actionSummary: string, risk: ToolRisk, };

export type ApprovalResolvedEvent = { approvalId: string, approved: boolean, };

/**
 * Body of `PUT /admin/approval-routing`: replaces the risks.
 */
export type ApprovalRoutingUpdate = { managerRisks: Array<ToolRisk>, };

/**
 * `GET /admin/approval-routing`.
 */
export type ApprovalRoutingView = {
/**
 * Empty: whom the agent acts for validates every action, as before.
 */
managerRisks: Array<ToolRisk>,
/**
 * Whether a manager is e-mailed when a request awaits them: only when
 * the operator configured a mail relay (ADR-060).
 */
notifiesByEmail: boolean, };

/**
 * Body of `POST /approvals/{id}/decision`.
 */
export type ApprovalVerdict = { approved: boolean, comment?: string, };

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
export type AssignmentScope = { "type": "tenant", } | { "type": "org_unit", orgUnitId: string, includeDescendants: boolean, } | { "type": "self", };

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

export type AuditActor = "user" | "agent" | "system";

/**
 * One entry of the audit trail. It names what changed, never a secret or
 * a message's content.
 */
export type AuditEntry = { id: number, occurredAt: string, actorType: AuditActor, actorUserId: string | null, onBehalfOfUserId: string | null, runId: string | null, action: string, resourceType: string | null, resourceId: string | null, outcome: AuditOutcome, reason: string | null, metadata: Record<string, unknown>, };

/**
 * `GET /admin/audit`, newest first.
 */
export type AuditLog = { entries: Array<AuditEntry>, };

export type AuditOutcome = "success" | "denied" | "failure";

/**
 * `GET /auth/options`: what the sign-in page may offer.
 */
export type AuthOptions = {
/**
 * An e-mail relay is configured: "forgotten password" can send a link.
 */
passwordReset: boolean, };

/**
 * As `skill_evaluations::CaseOutcome` stores it.
 */
export type CaseOutcomeView = "passed" | "failed" | "error";

export type CaseResultView = { outcome: CaseOutcomeView,
/**
 * Whether the agent loaded the version played.
 */
skillLoaded: boolean,
/**
 * Why no answer was judged, in the platform's words.
 */
error: string | null,
/**
 * Shown to whom the evaluation ran for only, as the next two.
 */
answer: string | null, issues: Array<string> | null, tools: Array<ToolCallView> | null, };

/**
 * A source the agent's answer relies on: an identifier returned by a tool.
 */
export type Citation = { type: string, id: string, label?: string, };

/**
 * `GET /admin/users/{id}/absence`.
 */
export type CollaboratorAbsence = {
/**
 * Declared and not ended, by the collaborator or an administrator.
 */
absence: Delegation | null,
/**
 * Whom the administrator may name: the collaborator's active
 * colleagues holding `approval:decide` on a unit or the company.
 */
candidates: Array<ApprovalPerson>, };

/**
 * An attribute a condition can read.
 */
export type ConditionAttribute = {
/**
 * e.g. `subject.attributes.contractType`.
 */
path: string, label: string, };

export type ConnectorStatus = "untested" | "ok" | "error" | "disabled";

/**
 * `POST /admin/connectors/{id}/test`, and `POST /admin/connectors/trial`
 * for a server not saved yet (ADR-050).
 */
export type ConnectorTest = { ok: boolean, tools: Array<TestedTool>,
/**
 * Why it could not start (without the token).
 */
error: string | null, };

/**
 * Body of `POST /admin/connectors/trial`: an MCP server tried before it is
 * saved, with the token typed in the form (ADR-050). Checked as when it is
 * added; nothing is stored.
 */
export type ConnectorTrial = { prefix: string, url: string,
/**
 * Absent or blank: none.
 */
token?: string, };

/**
 * A kind of connector the platform provides.
 */
export type ConnectorType = { type: string, name: string, description: string,
/**
 * Serves fictitious data, labelled as such.
 */
demo: boolean, };

/**
 * Body of `PATCH /admin/connectors/{id}`: only the fields sent change.
 */
export type ConnectorUpdate = { name?: string, enabled?: boolean, url?: string,
/**
 * A text replaces the token, `null` or blank removes it.
 */
token?: string | null,
/**
 * Replaces every tool's policy.
 */
toolPolicies?: { [key in string]: ToolPolicyChange },
/**
 * A REST API's: a text names the header its token goes in, `null` or
 * blank sends it as `Authorization: Bearer` (ADR-112).
 */
tokenHeader?: string | null,
/**
 * A REST API's: replaces every operation (ADR-112).
 */
operations?: Array<RestOperation>, };

export type ConnectorView = { id: string, type: string, name: string, enabled: boolean, status: ConnectorStatus, lastError: string | null,
/**
 * As stored: a policy runs cannot read makes the connector unavailable,
 * with `lastError` saying so, rather than hiding it here.
 */
toolPolicies: Record<string, ToolPolicy>, mcp: McpEndpoint | null, rest: RestApi | null, updatedAt: string, };

/**
 * `GET /admin/connectors`.
 */
export type Connectors = { instances: Array<ConnectorView>, types: Array<ConnectorType>, permissions: Array<DelegablePermission>, risks: Array<ToolRisk>,
/**
 * The server can seal tokens (`MIMORYA_MASTER_KEY`).
 */
secretsAvailable: boolean,
/**
 * The operator lets connectors reach its private network (private deployment).
 */
allowPrivateNetwork: boolean, };

/**
 * Connectors of the company in reach of the user that could not start:
 * their tools are missing from the run (ADR-048).
 */
export type ConnectorsUnavailable = {
/**
 * Their names, as the administration shows them.
 */
connectors: Array<string>, };

/**
 * Body of `PATCH /conversations/{id}`.
 */
export type ConversationChanges = { title?: string, archived?: boolean, };

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
 * Body of `POST /auth/login`.
 */
export type Credentials = { email: string, password: string, };

/**
 * A permission a tool may require: one an agent can hold.
 */
export type DelegablePermission = { permission: string, label: string, };

/**
 * An absence: from when until when the delegate stands in for the absent
 * person, deciding what they hold `approval:decide` for.
 */
export type Delegation = { absent: ApprovalPerson, delegate: ApprovalPerson, startsAt: string, endsAt: string,
/**
 * The administrator who declared it on the absent person's behalf
 * (ADR-078); `null` when they declared it themselves.
 */
declaredBy: ApprovalPerson | null, };

export type DeliveryOutcome = "started" | "limited" | "refused" | "failed";

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
 * A document deposited: its identifier and the passages its text gave.
 */
export type DocumentCreated = { id: string, pageCount: number | null, passages: number, };

export type DocumentItem = { id: string, title: string, fileName: string, kind: DocumentKind, scope: DocumentScope, orgUnitId: string | null,
/**
 * Name of the unit whose space it is in.
 */
orgUnit: string | null, sensitivity: Sensitivity, byteSize: number,
/**
 * Its pages, for a PDF.
 */
pageCount: number | null, createdAt: string,
/**
 * The reader may delete it.
 */
canDelete: boolean, };

/**
 * The kind of a deposited file.
 */
export type DocumentKind = "pdf" | "docx" | "markdown" | "text";

/**
 * `GET /documents`.
 */
export type DocumentList = {
/**
 * The latest documents read, a bounded number of them.
 */
documents: Array<DocumentItem>,
/**
 * Older documents are read too, beyond those listed.
 */
truncated: boolean,
/**
 * Where the reader may deposit.
 */
spaces: Array<DocumentSpaceOption>, quota: DocumentQuota | null,
/**
 * The largest file a deposit takes, in bytes (ADR-095).
 */
maxFileBytes: number, };

/**
 * The company's documents held, against its quota: shown to those who
 * deposit.
 */
export type DocumentQuota = { usedBytes: number, quotaBytes: number, };

/**
 * Whose space a document is in.
 */
export type DocumentScope = "tenant" | "org_unit";

/**
 * A space the reader may deposit into: the company's (no unit) or a unit's.
 */
export type DocumentSpaceOption = { unitId: string | null, name: string, };

/**
 * `POST /documents?name=…&unitId=…&sensitivity=…`, the file as body: its
 * name, the unit whose space receives it (the company's without one), its
 * sensitivity (`internal` without one).
 */
export type DocumentUpload = { name: string, unitId?: string, sensitivity?: Sensitivity, };

/**
 * Body of `POST /admin/identity-providers/domain-proofs`: the domain whose
 * record the company published, to read now.
 */
export type DomainProofRequest = { domain: string, };

/**
 * A domain the company's providers cover.
 */
export type DomainProofView = { domain: string,
/**
 * The TXT record to publish in the domain's DNS: where, and its value.
 */
recordName: string, recordValue: string,
/**
 * When the company last proved it; absent: never.
 */
verifiedAt: string | null, };

/**
 * A change done, with nothing else to return.
 */
export type Done = { ok: boolean, };

/**
 * Body of `PUT /admin/embedding`: the company's embedding model.
 */
export type EmbeddingChoice = { provider: string,
/**
 * Absent or blank: the provider's default model.
 */
model?: string, baseUrl?: string,
/**
 * Absent: the provider's starting value.
 */
minSimilarity?: number,
/**
 * Absent: the stored key is kept (same endpoint only).
 */
apiKey?: string, };

/**
 * A kind of embedding provider a company may choose.
 */
export type EmbeddingKind = { kind: string, requiresKey: boolean,
/**
 * Served at an endpoint the operator lists.
 */
listedEndpoint: boolean, defaultModel: string | null,
/**
 * Where to start the closeness a memory needs, for this kind.
 */
defaultMinSimilarity: number, };

/**
 * The company's embedding model.
 */
export type EmbeddingSettingsView = { provider: string, model: string, baseUrl: string | null, minSimilarity: number, keyStored: boolean,
/**
 * The stored configuration can serve (key readable, endpoint still listed…).
 */
usable: boolean,
/**
 * Why it cannot, when it cannot (French).
 */
problem: string | null,
/**
 * Active memories with a vector of this model.
 */
embedded: number,
/**
 * Active memories.
 */
memories: number,
/**
 * Document passages with a vector of this model (ADR-096).
 */
embeddedPassages: number,
/**
 * Document passages.
 */
passages: number,
/**
 * Why the last indexing failed, until one succeeds (French).
 */
indexingError: string | null, updatedAt: string, };

/**
 * `POST /admin/embedding/test`.
 */
export type EmbeddingTestResult = { ok: boolean, provider: string, model: string,
/**
 * The provider's refusal, as shown to users (never the key).
 */
error: string | null, };

/**
 * `GET /admin/embedding`.
 */
export type EmbeddingView = {
/**
 * The server can seal keys (`MIMORYA_MASTER_KEY`).
 */
secretsAvailable: boolean,
/**
 * `None`: the company's memory is searched by its words only.
 */
settings: EmbeddingSettingsView | null, providers: Array<EmbeddingKind>,
/**
 * Endpoints the operator lists for self-hosted servers.
 */
endpoints: Array<string>, };

/**
 * Where a permission is checked so far.
 */
export type Enforcement = "not_yet" | "holder" | "per_resource";

/**
 * The body of every error response.
 */
export type ErrorBody = { error: ErrorDetail, };

export type ErrorDetail = { code: ErrorKind, message: string,
/**
 * What the client may need beyond the message: the invalid fields
 * (`{ issues: [{ path, message }] }`), or why access is held back
 * (`{ reason: "password_change_required" }`).
 */
details?: unknown, };

export type ErrorKind = "validation" | "unauthenticated" | "forbidden" | "not_found" | "conflict" | "budget_exceeded" | "unavailable" | "request" | "rate_limited" | "internal";

export type EvaluatedCaseView = { name: string, request: string, criteria: Array<string>,
/**
 * For the version evaluated; `None` while not played.
 */
result: CaseResultView | null,
/**
 * For the active version played beside it.
 */
reference: CaseResultView | null, regression: boolean, };

/**
 * Body of `POST /admin/skills/{id}/evaluations`: the version tried.
 */
export type EvaluationRequest = { version: number, };

export type EvaluationStatus = "queued" | "running" | "completed" | "failed";

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

export type HttpMethod = "GET" | "POST" | "PUT" | "PATCH" | "DELETE";

/**
 * Body of `PATCH /admin/identity-providers/{id}`: absent fields are
 * unchanged. Kind, issuer and client are not: another provider is declared.
 */
export type IdentityProviderUpdate = { name?: string,
/**
 * A new secret replaces the stored one.
 */
clientSecret?: string,
/**
 * SAML: the provider's metadata anew (its certificates rolled over).
 */
metadata?: string, domains?: Array<string>, enabled?: boolean, required?: boolean, createAccounts?: boolean, };

export type IdentityProviderView = { id: string,
/**
 * `microsoft`, `google`, `oidc` or `saml`.
 */
kind: string, name: string, issuer: string, clientId: string,
/**
 * E-mail domains whose accounts sign in through it.
 */
domains: Array<string>, enabled: boolean,
/**
 * Its domains' accounts sign in through it alone, but for the
 * company's administrators of its sign-in (ADR-066).
 */
required: boolean,
/**
 * It creates the account of a person of its domains at their first
 * sign-in through the company's link (ADR-067).
 */
createAccounts: boolean,
/**
 * The company's sign-in link that names it, to hand out (ADR-067).
 */
signInLink: string, secretStored: boolean, createdAt: string, updatedAt: string, };

/**
 * `GET /admin/identity-providers`.
 */
export type IdentityProvidersView = {
/**
 * The server can seal client secrets (`MIMORYA_MASTER_KEY`).
 */
secretsAvailable: boolean,
/**
 * The address to register at the provider for the return of a sign-in.
 */
redirectUri: string,
/**
 * SAML: the company's own entity ID (identifier) and the address
 * responses are posted to, to register at the provider.
 */
samlEntityId: string, samlAcsUrl: string, providers: Array<IdentityProviderView>,
/**
 * Every domain the providers cover, and its proof (ADR-069).
 */
domains: Array<DomainProofView>, };

export type InboundHook = { id: string, title: string,
/**
 * Sent as the author's, followed by what was posted.
 */
instruction: string,
/**
 * Paused when `false`: a delivery answers as to an unknown hook.
 */
enabled: boolean,
/**
 * Where a system posts, under the API (`/api` from outside).
 */
path: string,
/**
 * The same, in full, from the application's public origin.
 */
url: string, lastDelivery: LastDelivery | null,
/**
 * Where the last run a delivery started is, while that conversation
 * exists.
 */
lastConversationId: string | null, };

/**
 * `GET /inbound-hooks`: the reader's own, oldest first.
 */
export type InboundHookList = { hooks: Array<InboundHook>,
/**
 * How many one may keep.
 */
maxHooks: number,
/**
 * Deliveries a hook takes in an hour.
 */
maxPerHour: number,
/**
 * What a delivery may post, at most.
 */
maxBodyBytes: number, };

/**
 * `POST /inbound-hooks` and `POST /inbound-hooks/{id}/token`: the token,
 * shown this once.
 */
export type InboundHookToken = { id: string, path: string, url: string,
/**
 * Sent by the system as `Authorization: Bearer <token>`.
 */
token: string, };

/**
 * Body of `PATCH /inbound-hooks/{id}`: what changes; the rest stays.
 */
export type InboundHookUpdate = { title?: string, instruction?: string, enabled?: boolean, };

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
 * What became of the last delivery.
 */
export type LastDelivery = { at: string, outcome: DeliveryOutcome,
/**
 * Why it did not run, as the platform says it.
 */
problem: string | null, };

/**
 * What became of the last occurrence.
 */
export type LastOccurrence = { at: string, outcome: OccurrenceOutcome,
/**
 * Why it did not run, as the platform says it.
 */
problem: string | null, };

/**
 * Body of `POST /auth/invitation/accept` and `POST /auth/password-reset/complete`:
 * the password chosen with a link.
 */
export type LinkPassword = { token: string, password: string, };

/**
 * Body of `POST /auth/invitation` and `POST /auth/password-reset/link`: the
 * link to open.
 */
export type LinkToken = { token: string, };

/**
 * Body of `PUT /admin/llm` and `PUT /admin/llm/verification`: a provider of
 * the company's own.
 */
export type LlmChoice = { provider: string,
/**
 * Absent or blank: the provider's default model.
 */
model?: string, baseUrl?: string,
/**
 * Absent: the stored key is kept (same endpoint only).
 */
apiKey?: string, };

/**
 * A provider of the company's own, for its runs or for reviews.
 */
export type LlmSettingsView = { provider: string, model: string, baseUrl: string | null, keyStored: boolean,
/**
 * Present in the price table: costs are estimated.
 */
priced: boolean,
/**
 * The stored configuration can serve (key readable, endpoint still listed…).
 */
usable: boolean,
/**
 * Why it cannot, when it cannot (French).
 */
problem: string | null, updatedAt: string, };

/**
 * `POST /admin/llm/test` and `POST /admin/llm/verification/test`.
 */
export type LlmTestResult = { ok: boolean, provider: string, model: string,
/**
 * The provider's refusal, as shown to users (never the key).
 */
error: string | null, };

/**
 * `GET /admin/llm`.
 */
export type LlmView = {
/**
 * The server can seal keys (`MIMORYA_MASTER_KEY`).
 */
secretsAvailable: boolean, platform: PlatformModel | null, settings: LlmSettingsView | null,
/**
 * The provider the company chose apart to review its answers (ADR-108);
 * without it, or while it cannot be used, its runs' model reviews them.
 */
verification: LlmSettingsView | null, providers: Array<ProviderKind>,
/**
 * Endpoints the operator lists for self-hosted providers.
 */
endpoints: Array<string>, };

/**
 * Body of `POST /memory`.
 */
export type ManualMemory = { space: MemorySpace, kind: ManualMemoryKind, title: string, content: string, };

/**
 * What a user may write by hand (episodes come from runs).
 */
export type ManualMemoryKind = "preference" | "fact" | "procedure" | "decision" | "lesson";

/**
 * An MCP server reached over HTTPS (editable from the administration).
 */
export type McpEndpoint = {
/**
 * Its tools are named `mcp_<prefix>_<tool>`.
 */
prefix: string, url: string, tokenStored: boolean, };

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
mustChangePassword: boolean,
/**
 * False when the account signs in through its company's identity
 * provider only (ADR-064): it has no password to change.
 */
hasPassword: boolean, };

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
 * Body of `POST /memory/{id}/review`: the decision on a proposed memory.
 */
export type MemoryReview = { decision: ReviewDecision, };

/**
 * The space a memory belongs to.
 */
export type MemoryScope = "user" | "org_unit" | "tenant" | "experience";

/**
 * Body of `PUT /admin/memory-settings`.
 */
export type MemorySettingsUpdate = { captureLessons: boolean, };

/**
 * `GET /admin/memory-settings`.
 */
export type MemorySettingsView = {
/**
 * A lesson is drawn from each run that used tools and proposed in the
 * personal memory of whom it served.
 */
captureLessons: boolean, };

/**
 * Where a memory comes from: written by hand, or by a run of the assistant.
 */
export type MemorySource = { type: string, id?: string, label?: string, };

/**
 * Where a user adds a memory: their own, their team's, the company's.
 */
export type MemorySpace = "personal" | "team" | "company";

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
evidence?: Array<Citation>, verification?: Verification,
/**
 * What the message is to its mission, in a mission's conversation
 * (ADR-089).
 */
mission?: MissionPart,
/**
 * The inbound hook whose delivery sent the message, as its author's
 * (ADR-109): its instruction, then what was posted, enclosed.
 */
inboundHookId?: string, };

export type MessageRole = "user" | "assistant";

export type Mission = { id: string, title: string, objective: string, successCriteria: Array<string>, status: MissionStatus,
/**
 * Where its stages run, and its answer is given.
 */
conversationId: string,
/**
 * Stages started, and how many it may start.
 */
stages: number, maxStages: number,
/**
 * What its stages cost so far, and how much they may.
 */
costMicros: number, maxCostMicros: number,
/**
 * Its final answer, once done.
 */
result: string | null,
/**
 * How its final answer was reviewed against its criteria.
 */
verification: Verification | null,
/**
 * Why it stopped unfinished.
 */
error: string | null, createdAt: string, finishedAt: string | null, };

/**
 * `GET /missions`: the reader's own, newest first.
 */
export type MissionList = { missions: Array<Mission>,
/**
 * How many may be under way at once, per person.
 */
maxUnderWay: number, };

/**
 * What a message is to its mission (ADR-089).
 */
export type MissionPart = "brief" | "resumption" | "progress";

/**
 * Body of `PUT /admin/mission-settings`: checked against the platform's
 * bounds by the service, which says them in its refusal.
 */
export type MissionSettingsUpdate = { maxStages: number, maxCostMicros: number, };

/**
 * `GET /admin/mission-settings`.
 */
export type MissionSettingsView = {
/**
 * How many stages the missions entrusted from now on may take.
 */
maxStages: number,
/**
 * What they may cost, in micro-dollars.
 */
maxCostMicros: number,
/**
 * Whether these are the company's own limits, not the platform's.
 */
own: boolean,
/**
 * The platform's limits, which apply without the company's own.
 */
platformMaxStages: number, platformMaxCostMicros: number,
/**
 * What a company may set: stages up to this many…
 */
maxStagesCeiling: number,
/**
 * …and a cost from this much…
 */
minCostMicros: number,
/**
 * …to this much.
 */
maxCostCeilingMicros: number, };

export type MissionStatus = "in_progress" | "awaiting_approval" | "done" | "failed" | "cancelled";

/**
 * Body of `POST /admin/users`: the account is invited (ADR-027).
 */
export type NewAccount = { email: string, displayName: string, jobTitle?: string, orgUnitId?: string,
/**
 * Absent: `internal`.
 */
clearance?: Sensitivity, };

/**
 * Body of `POST /admin/connectors`: an MCP server over HTTPS. The prefix,
 * address and token are checked by the platform (ADR-025).
 */
export type NewConnector = { name: string, prefix: string, url: string,
/**
 * Absent or blank: none.
 */
token?: string,
/**
 * Absent: enabled.
 */
enabled?: boolean, };

/**
 * Body of `POST /conversations`.
 */
export type NewConversation = { title?: string, };

/**
 * Body of `POST /admin/identity-providers`. What designates the provider
 * depends on its kind: the directory (tenant ID) of a Microsoft Entra ID,
 * nothing for Google, the issuer's address for OpenID Connect, each with
 * the company's client and its secret; a SAML provider's metadata alone.
 */
export type NewIdentityProvider = { kind: string,
/**
 * Absent or blank: the kind's name ("Microsoft 365", "Google Workspace").
 */
name?: string, directoryId?: string, issuer?: string,
/**
 * Absent for SAML.
 */
clientId?: string, clientSecret?: string,
/**
 * SAML: the provider's metadata (XML).
 */
metadata?: string, domains: Array<string>,
/**
 * Absent: enabled.
 */
enabled?: boolean,
/**
 * Absent: not required (ADR-066).
 */
required?: boolean,
/**
 * Absent: no account created (ADR-067).
 */
createAccounts?: boolean, };

/**
 * Body of `POST /inbound-hooks`.
 */
export type NewInboundHook = { title: string, instruction: string,
/**
 * Paused from the start when `false`; receives by default.
 */
enabled?: boolean, };

/**
 * Body of `POST /conversations/{id}/messages`.
 */
export type NewMessage = { content: string, };

/**
 * Body of `POST /missions`.
 */
export type NewMission = { title: string, objective: string, successCriteria: Array<string>, };

/**
 * Body of `POST /admin/org-units`. The type is checked by the route, which
 * names an unknown one in French.
 */
export type NewOrgUnit = { parentId: string, name: string, type: OrgUnitType, };

/**
 * Body of `POST /admin/policies`: a deny policy.
 */
export type NewPolicy = { description: string, permissions: Array<string>,
/**
 * `{ all }`, `{ any }`, `{ not }` or a comparison, as `PolicyView.condition`.
 */
condition: Record<string, unknown>,
/**
 * Absent: enabled.
 */
enabled?: boolean, };

/**
 * Body of `POST /admin/connectors/rest`: a REST API and its operations,
 * checked as a whole by the platform (ADR-112).
 */
export type NewRestConnector = { name: string, prefix: string, url: string,
/**
 * Absent or blank: `Authorization: Bearer`.
 */
tokenHeader?: string,
/**
 * Absent or blank: none.
 */
token?: string,
/**
 * Absent: enabled.
 */
enabled?: boolean, operations: Array<RestOperation>, };

/**
 * Body of `POST /admin/roles`: a custom role and its permissions.
 */
export type NewRole = { name: string,
/**
 * Absent: none.
 */
description?: string, permissions: Array<string>, };

/**
 * Body of `POST /scheduled-tasks`.
 */
export type NewScheduledTask = { title: string, instruction: string, weekdays: Array<number>, time: string, timeZone: string,
/**
 * Paused from the start when `false`; runs by default.
 */
enabled?: boolean, };

/**
 * Body of `POST /admin/skills`: a skill and its first version, a draft.
 */
export type NewSkill = { name: string, description: string, content: string, changelog?: string,
/**
 * What an answer that follows the version must meet, one per line
 * (ADR-106): blank ones are left out, and 8 at most may remain, of 300
 * characters at most (`skill_quality::criteria_problem`).
 */
qualityCriteria?: Array<string>, };

/**
 * Body of `POST /admin/skills/{id}/versions`: a new draft version.
 */
export type NewSkillVersion = { content: string, changelog?: string,
/**
 * What an answer that follows the version must meet, one per line
 * (ADR-106): blank ones are left out, and 8 at most may remain, of 300
 * characters at most (`skill_quality::criteria_problem`); `[]` for
 * none. Absent: those of the version active.
 */
qualityCriteria?: Array<string>, };

/**
 * `GET /account/notifications`, and the body of its `PUT`: each kind of
 * e-mail, received or not. Invitations and password resets are always
 * sent.
 */
export type NotificationSettings = {
/**
 * The requests awaiting their validation, as a manager or standing in
 * for one, and the reminders before they expire.
 */
approvals: boolean,
/**
 * Their scheduled tasks' runs, missions' stages and hooks' runs
 * awaiting it.
 */
ownRuns: boolean,
/**
 * The end of their missions.
 */
missionEnds: boolean, };

export type OccurrenceOutcome = "started" | "still_running" | "missed" | "refused" | "failed";

/**
 * Body of `POST /admin/connectors/rest/import`: an OpenAPI 3 description
 * in JSON, as written.
 */
export type OpenApiDescription = { document: string, };

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

/**
 * Body of `PATCH /admin/org-units/{id}`: only the fields sent change.
 */
export type OrgUnitUpdate = { name?: string, type?: OrgUnitType, parentId?: string,
/**
 * `null`: no lead.
 */
leadUserId?: string | null, };

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

/**
 * Where a parameter goes: the path (`{name}`), the query, the JSON body.
 */
export type ParameterLocation = "path" | "query" | "body";

export type ParameterType = "string" | "integer" | "number" | "boolean";

/**
 * Body of `POST /account/password`.
 */
export type PasswordUpdate = { currentPassword: string, newPassword: string, };

export type PermissionView = { permission: string, label: string,
/**
 * `false`: the agent can never exercise it.
 */
delegable: boolean, enforcement: Enforcement, };

/**
 * The provider the platform offers every company.
 */
export type PlatformModel = { provider: string, model: string, priced: boolean, };

/**
 * `GET /admin/policies`.
 */
export type Policies = { policies: Array<PolicyView>, permissions: Array<RestrictablePermission>,
/**
 * Domains a policy may restrict as a whole (`domain:*`).
 */
domains: Array<string>, attributes: Array<ConditionAttribute>, };

export type PolicyEffect = "allow" | "deny";

/**
 * Body of `PATCH /admin/policies/{id}`: only the fields sent change.
 */
export type PolicyUpdate = { description?: string, permissions?: Array<string>, condition?: Record<string, unknown>, enabled?: boolean, };

export type PolicyView = { id: string, description: string, effect: PolicyEffect, permissions: Array<string>,
/**
 * The condition as stored: `{ all }`, `{ any }`, `{ not }` or a comparison.
 */
condition: Record<string, unknown>, enabled: boolean,
/**
 * Only deny policies are written from the administration (ADR-022).
 */
editable: boolean, updatedAt: string, };

/**
 * A kind of provider a company may choose.
 */
export type ProviderKind = { kind: string, requiresKey: boolean,
/**
 * Served at an endpoint the operator lists.
 */
listedEndpoint: boolean, defaultModel: string | null, };

/**
 * `POST /auth/password-reset/link`: the account whose password is reset.
 */
export type ResetDetails = { email: string, expiresAt: string, };

/**
 * Body of `POST /auth/password-reset`: the address to send a link to.
 */
export type ResetLinkRequest = { email: string, };

/**
 * `POST /admin/users/{id}/password-link`: the administrator never sees the link.
 */
export type ResetLinkSent = { expiresAt: string, };

/**
 * A REST API as stored, and whether a token is.
 */
export type RestApi = {
/**
 * Its tools are named `rest_<prefix>_<operation>`.
 */
prefix: string, url: string,
/**
 * `null`: the token goes as `Authorization: Bearer`.
 */
tokenHeader: string | null, tokenStored: boolean, operations: Array<RestOperation>, };

/**
 * The operations the description offers, each one a declaration accepts:
 * its paths in the order written, the methods of each in this order: GET,
 * PUT, POST, DELETE, PATCH.
 */
export type RestImport = {
/**
 * `null`: the description gives no complete address.
 */
url: string | null, operations: Array<RestOperation>,
/**
 * What was left aside or changed, and why.
 */
notes: Array<string>, };

/**
 * One operation of the API, which becomes the tool `rest_<prefix>_<name>`.
 */
export type RestOperation = { name: string, title: string,
/**
 * What the model reads of the tool, after the API's prefix.
 */
description: string, method: HttpMethod,
/**
 * Follows the API's address; `{name}` stands for a path parameter.
 */
path: string, parameters: Array<RestParameter>, };

/**
 * What the model may give an operation.
 */
export type RestParameter = { name: string, in: ParameterLocation, type: ParameterType,
/**
 * A path parameter always is.
 */
required?: boolean, description?: string, };

/**
 * A permission a policy may restrict: a business one the platform checks.
 */
export type RestrictablePermission = { permission: string, label: string,
/**
 * Checked resource by resource: a condition on the resource applies.
 */
perResource: boolean, };

/**
 * What becomes of a proposed memory: kept as active, or archived.
 */
export type ReviewDecision = "approve" | "reject";

/**
 * Body of `POST /admin/users/{id}/assignments`.
 */
export type RoleGrant = { roleId: string, scope: AssignmentScope,
/**
 * ISO 8601; absent, the role is held until revoked.
 */
expiresAt?: string, };

/**
 * Body of `PATCH /admin/roles/{id}`: only the fields sent change.
 */
export type RoleUpdate = { name?: string, description?: string, permissions?: Array<string>, };

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
export type RunDetail = { run: RunView, tools: Array<ToolInvocation>, approvals: Array<Approval>, events: Array<RunEventRecord>,
/**
 * The sub-agents it handed tasks to, in order (ADR-098).
 */
subRuns: Array<RunListItem>, };

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
 * A run in the list of the user's runs, or of a run's sub-agents.
 */
export type RunListItem = { id: string, conversationId: string | null, objective: string, agentProfile: AgentProfile, status: RunStatus, usage: RunUsage, error: string | null, createdAt: string, finishedAt: string | null, };

/**
 * The run waits for a worker to execute it (ADR-085): what it does next
 * follows once one claims it.
 */
export type RunQueued = Record<symbol, never>;

/**
 * A run of a conversation, listed with its messages.
 */
export type RunRef = { id: string, status: RunStatus, createdAt: string, };

/**
 * The run was taken up again from its last saved step after a stop of the
 * service (ADR-053).
 */
export type RunResumed = {
/**
 * Read-only actions under way when the service stopped, whose results
 * were lost (tool names).
 */
interrupted: Array<string>, };

export type RunStarted = { objective: string, };

export type RunStatus = "queued" | "running" | "awaiting_approval" | "completed" | "failed" | "cancelled";

/**
 * One event of the stream, by name.
 */
export type RunStreamEvent = { "type": "run_started", "data": RunStarted } | { "type": "connectors_unavailable", "data": ConnectorsUnavailable } | { "type": "assistant_delta", "data": AssistantDelta } | { "type": "assistant_message", "data": AssistantMessage } | { "type": "tool_call", "data": ToolCallEvent } | { "type": "tool_result", "data": ToolResultEvent } | { "type": "approval_requested", "data": ApprovalRequestedEvent } | { "type": "approval_resolved", "data": ApprovalResolvedEvent } | { "type": "verification", "data": VerificationEvent } | { "type": "usage", "data": UsageEvent } | { "type": "run_resumed", "data": RunResumed } | { "type": "run_queued", "data": RunQueued } | { "type": "run_finished", "data": RunFinished };

/**
 * What a run has consumed so far (`agent_runs.usage`).
 */
export type RunUsage = { steps: number, toolCalls: number, inputTokens: number, outputTokens: number,
/**
 * Estimated cost, in millionths of the currency unit.
 */
costMicros: number, };

export type RunView = { id: string, conversationId: string | null,
/**
 * The run that handed it its task, for a sub-agent's (ADR-098).
 */
parentRunId: string | null, agentProfile: AgentProfile, objective: string, status: RunStatus, autonomyLevel: string, usage: RunUsage, budget: RunBudget, verification: Verification | null, error: string | null, createdAt: string, finishedAt: string | null, };

export type ScheduledTask = { id: string, title: string,
/**
 * Sent as the author's, each time.
 */
instruction: string,
/**
 * ISO days of the week: 1 is Monday.
 */
weekdays: Array<number>,
/**
 * `HH:MM`, in `timeZone`.
 */
time: string, timeZone: string,
/**
 * Paused when `false`.
 */
enabled: boolean, nextRunAt: string | null, lastOccurrence: LastOccurrence | null,
/**
 * Where the last run an occurrence started is, while that conversation
 * exists: an occurrence does not start while that run is under way.
 */
lastConversationId: string | null, };

/**
 * `GET /scheduled-tasks`: the reader's own, oldest first.
 */
export type ScheduledTaskList = { tasks: Array<ScheduledTask>,
/**
 * How many one may keep.
 */
maxTasks: number, };

/**
 * Body of `PATCH /scheduled-tasks/{id}`: what changes; the rest stays.
 */
export type ScheduledTaskUpdate = { title?: string, instruction?: string, weekdays?: Array<number>, time?: string, timeZone?: string, enabled?: boolean, };

/**
 * Highest sensitivity of the data an account may read.
 */
export type Sensitivity = "public" | "internal" | "confidential" | "restricted";

/**
 * Body of `PUT /admin/skills/{id}/activation`: the version made active.
 */
export type SkillActivation = { version: number,
/**
 * Made active against the regressions its last evaluation found
 * (ADR-104). Absent: not confirmed.
 */
confirmed?: boolean, };

/**
 * `GET /admin/skills/{id}`: the skill and its versions, the latest first,
 * and its evaluation cases in the order they were written (ADR-103).
 */
export type SkillDetail = { skill: SkillView, versions: Array<SkillVersionView>, cases: Array<SkillEvalCaseView>, };

/**
 * Body of `POST /admin/skills/{id}/cases` and of
 * `PUT /admin/skill-cases/{id}`: the case as a whole.
 */
export type SkillEvalCaseInput = { name: string, request: string,
/**
 * One per line sent, at most `MAX_CRITERIA_LINES`; blank ones are left
 * out, and 1 to 8 must remain, of 300 characters at most
 * (`skill_eval_cases::case_problem`).
 */
criteria: Array<string>, };

export type SkillEvalCaseView = { id: string, name: string,
/**
 * What the case asks, as a person would.
 */
request: string,
/**
 * What a good answer does, one criterion each.
 */
criteria: Array<string>, createdBy: string | null, updatedAt: string, };

/**
 * `GET /admin/skill-evaluations/{id}`.
 */
export type SkillEvaluationDetail = { evaluation: SkillEvaluationView, cases: Array<EvaluatedCaseView>,
/**
 * Whether the answers, issues and tools are shown: to whom the
 * evaluation ran for.
 */
detailsShown: boolean, };

export type SkillEvaluationView = { id: string, version: number,
/**
 * The active version played beside it, when another one was evaluated.
 */
referenceVersion: number | null, status: EvaluationStatus, failure: string | null, cases: number,
/**
 * Cases played so far for the version evaluated.
 */
played: number, passed: number, referencePassed: number | null,
/**
 * Cases the active version passed and the evaluated one did not.
 */
regressions: number, launchedBy: string | null, costMicros: number, createdAt: string, startedAt: string | null, finishedAt: string | null, };

/**
 * `GET /admin/skills/{id}/evaluations`: the latest first.
 */
export type SkillEvaluations = { evaluations: Array<SkillEvaluationView>, };

/**
 * `GET /admin/skills`.
 */
export type SkillList = { skills: Array<SkillView>, };

/**
 * Body of `POST /admin/skills/{id}/rejection`: the proposal turned down.
 */
export type SkillRejection = { version: number, };

export type SkillStatus = "draft" | "active" | "retired";

/**
 * Body of `PATCH /admin/skills/{id}`.
 */
export type SkillUpdate = { description: string, };

/**
 * `POST /admin/skills/{id}/versions`.
 */
export type SkillVersionCreated = { version: number, };

export type SkillVersionStatus = "draft" | "candidate" | "active" | "rejected" | "retired";

export type SkillVersionView = { version: number, status: SkillVersionStatus,
/**
 * The procedure handed to the agent, in Markdown.
 */
content: string, changelog: string,
/**
 * What an answer that follows it must meet (ADR-106).
 */
qualityCriteria: Array<string>,
/**
 * Proposed by the agent for its author, who validated it (ADR-100).
 */
proposed: boolean,
/**
 * What a proposal of a new skill said the skill covers.
 */
proposedDescription: string | null, createdBy: string | null, createdAt: string,
/**
 * Who made it active last, and when.
 */
approvedBy: string | null, approvedAt: string | null, };

export type SkillView = { id: string, name: string, description: string, status: SkillStatus, activeVersion: number | null, latestVersion: number | null,
/**
 * The agent's proposals awaiting a decision (ADR-100).
 */
proposals: number, updatedAt: string, };

/**
 * `POST /auth/sso/discover`.
 */
export type SsoDiscovery = {
/**
 * Absent: the password only.
 */
provider: SsoProvider | null, };

/**
 * `POST /auth/sso/link`: the company the link signs in to, and its provider.
 */
export type SsoLink = { company: string, provider: SsoProvider,
/**
 * A first sign-in creates the account of a person of its domains.
 */
createsAccounts: boolean, };

/**
 * Body of `POST /auth/sso/link` and `POST /auth/sso/link/start`: the
 * provider the company's sign-in link names.
 */
export type SsoLinkLookup = { provider: string, };

/**
 * Body of `POST /auth/sso/discover` and `POST /auth/sso/start`.
 */
export type SsoLookup = { email: string, };

/**
 * A provider the sign-in page offers.
 */
export type SsoProvider = {
/**
 * `microsoft`, `google` or `oidc`.
 */
kind: string,
/**
 * As the button names it: "Microsoft 365".
 */
name: string,
/**
 * The account signs in through it alone, unless it administers the
 * company's sign-in (ADR-066).
 */
required: boolean, };

/**
 * `POST /auth/sso/start` and `POST /auth/sso/link/start`: where the
 * browser goes to sign in.
 */
export type SsoStart = { redirectUrl: string, };

/**
 * What an account's status may be set to (an invitation is issued, not set).
 */
export type StatusChange = "active" | "disabled";

/**
 * `POST /admin/users/{id}/password`: shown once, replaced at next sign-in.
 */
export type TemporaryPassword = { temporaryPassword: string, };

/**
 * A tool a server declared, with the policy in force.
 */
export type TestedTool = { name: string, title: string,
/**
 * As the server declares it.
 */
description: string,
/**
 * An administrator's replacement, as written: the model reads it after
 * the server's name (ADR-057).
 */
replacedDescription: string | null, enabled: boolean, risk: ToolRisk, permissions: Array<string>, };

export type ToolCallEvent = { toolCallId: string, toolName: string, title: string, decision: ToolDecision, reason?: string,
/**
 * The tool's risk when it was called (an unknown tool has none).
 */
risk?: ToolRisk, };

/**
 * As the evaluation's `ToolTrace` stores it.
 */
export type ToolCallView = { name: string, summary: string, isError: boolean, };

/**
 * The policy's decision on a tool call.
 */
export type ToolDecision = "allow" | "require_approval" | "deny";

/**
 * A tool call of a run, with the decision taken on it.
 */
export type ToolInvocation = { id: string, toolName: string, decision: ToolDecision, decisionReason: string | null, status: string, outputSummary: string | null, durationMs: number | null, createdAt: string, };

/**
 * An administrator's policy for one tool, as stored. Every field is
 * optional: absent or `null`, the tool's own declaration applies.
 */
export type ToolPolicy = { enabled?: boolean | null, permissions?: Array<string> | null, risk?: ToolRisk | null, requiresApproval?: boolean | null,
/**
 * What the model reads of the tool, after the server's name, instead of
 * the server's description (ADR-057).
 */
description?: string | null, };

/**
 * One tool's policy as an administrator writes it: absent fields keep the
 * tool's own declaration.
 */
export type ToolPolicyChange = { enabled?: boolean, risk?: ToolRisk, permissions?: Array<string>,
/**
 * Absent or blank: the server's own description (ADR-057).
 */
description?: string, };

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

export type UsageKind = "llm" | "embedding" | "tool";

/**
 * Usage of one model, summed over the period.
 */
export type UsageRow = { provider: string, model: string, kind: UsageKind, calls: number, inputTokens: number, outputTokens: number,
/**
 * Estimated cost, in millionths of the currency unit.
 */
costMicros: number, };

/**
 * `GET /admin/usage`.
 */
export type UsageSummary = {
/**
 * Start of the period.
 */
since: string, rows: Array<UsageRow>, };

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
