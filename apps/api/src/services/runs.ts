/**
 * Orchestrates agent runs for conversations: creation, background execution,
 * persistence of outcomes, and resumption after human validation.
 */
import {
  AgentRuntime,
  buildSystemPrompt,
  buildUserTurn,
  createPlatformTools,
  DEFAULT_BUDGET,
  ToolRegistry,
  type ApprovalDecision,
  type LlmMessage,
  type ModelRouter,
  type PriceTable,
  type RunCheckpoint,
  type RunOutcome,
} from '@mimorya/agent';
import {
  AUTONOMY_LEVELS,
  DomainError,
  memoryReadFilter,
  NotFoundError,
  sensitivityRank,
  type AutonomyLevel,
} from '@mimorya/core';
import {
  addMessage,
  appendAudit,
  createRun,
  decideApproval,
  expireApproval,
  getApproval,
  getConversation,
  listActiveSkills,
  listApprovalsForRun,
  listMessages,
  listRunsForConversation,
  lockRun,
  markMemoryUsed,
  searchMemory,
  updateConversation,
  updateRun,
  withTenant,
  type Database,
  type RunRow,
} from '@mimorya/db';
import type { FastifyBaseLogger } from 'fastify';
import type { AccessContext, AccessService, AuthIdentity } from './access';
import type { ConnectorService } from './connectors';
import { createDbPorts } from './db-ports';
import type { RunEventBus } from './event-bus';

export interface RunServiceDeps {
  db: Database;
  router: ModelRouter | null;
  prices: PriceTable;
  access: AccessService;
  connectors: ConnectorService;
  bus: RunEventBus;
  logger: FastifyBaseLogger;
  assistantName: string;
  llmJudge: boolean;
  now?: () => Date;
}

const HISTORY_LIMIT = 20;
const DEFAULT_TITLE = 'Nouvelle conversation';

export class RunService {
  private readonly inflight = new Set<Promise<void>>();

  constructor(private readonly deps: RunServiceDeps) {}

  get llmAvailable(): boolean {
    return this.deps.router !== null;
  }

  /** Resolves when no run is executing in the background (tests, shutdown). */
  async idle(): Promise<void> {
    while (this.inflight.size > 0) await Promise.allSettled([...this.inflight]);
  }

  async startConversationTurn(identity: AuthIdentity, conversationId: string, text: string) {
    if (!this.deps.router) {
      throw new DomainError(
        'unavailable',
        'Aucun fournisseur de modèle n’est configuré. Renseignez LLM_PROVIDER et la clé correspondante.',
      );
    }
    const access = await this.deps.access.context(identity);
    access.require('agent:use');
    const autonomy = this.autonomyLevel(access);

    const { run, history, userMessageId } = await withTenant(
      this.deps.db,
      identity.tenantId,
      async (tx) => {
        const conversation = await getConversation(tx, conversationId);
        if (!conversation || conversation.ownerUserId !== identity.userId) {
          throw new NotFoundError('Conversation introuvable');
        }
        const [latest] = await listRunsForConversation(tx, conversationId, 1);
        if (latest && (latest.status === 'running' || latest.status === 'awaiting_approval')) {
          throw new DomainError(
            'conflict',
            'Une demande est déjà en cours dans cette conversation : attendez sa fin ou sa validation.',
          );
        }
        const previous = await listMessages(tx, conversationId, HISTORY_LIMIT);
        const userMessage = await addMessage(tx, {
          tenantId: identity.tenantId,
          conversationId,
          role: 'user',
          content: text,
          authorUserId: identity.userId,
        });
        if (conversation.title === DEFAULT_TITLE) {
          await updateConversation(tx, conversationId, { title: text.slice(0, 80) });
        }
        const created = await createRun(tx, {
          tenantId: identity.tenantId,
          conversationId,
          initiatedByUserId: identity.userId,
          onBehalfOfUserId: identity.userId,
          objective: text,
          autonomyLevel: autonomy,
          budget: DEFAULT_BUDGET,
        });
        await appendAudit(tx, {
          tenantId: identity.tenantId,
          actorType: 'user',
          actorUserId: identity.userId,
          runId: created.id,
          action: 'agent.run.started',
          outcome: 'success',
          resourceType: 'conversation',
          resourceId: conversationId,
        });
        return { run: created, history: toHistory(previous), userMessageId: userMessage.id };
      },
    );

    this.track(this.execute(access, run, { kind: 'start', history, text }));
    return { runId: run.id, messageId: userMessageId };
  }

  async decide(identity: AuthIdentity, approvalId: string, approved: boolean, comment?: string) {
    const access = await this.deps.access.context(identity);
    const outcome = await withTenant(this.deps.db, identity.tenantId, async (tx) => {
      const approval = await getApproval(tx, approvalId);
      // MVP rule: a user validates the actions requested on their own behalf.
      if (!approval || approval.requestedForUserId !== identity.userId) {
        throw new NotFoundError('Demande de validation introuvable');
      }
      // Concurrent decisions on the same run are serialised: exactly one of them
      // observes the last pending approval and resumes the run.
      const run = await lockRun(tx, approval.runId);
      const expired =
        approval.status === 'pending' && !!approval.expiresAt && approval.expiresAt <= new Date();
      let decided = null;
      if (expired) {
        await expireApproval(tx, approvalId);
      } else {
        decided = await decideApproval(tx, {
          id: approvalId,
          approved,
          decidedByUserId: identity.userId,
          comment: comment ?? null,
        });
        if (!decided) throw new DomainError('conflict', 'Cette demande a déjà été traitée');
      }
      await appendAudit(tx, {
        tenantId: identity.tenantId,
        actorType: 'user',
        actorUserId: identity.userId,
        runId: approval.runId,
        action: expired ? 'approval.expired' : approved ? 'approval.approved' : 'approval.rejected',
        outcome: expired ? 'failure' : 'success',
        resourceType: 'approval',
        resourceId: approvalId,
        metadata: { toolName: approval.toolName, risk: approval.risk },
      });
      const all = await listApprovalsForRun(tx, approval.runId);
      const resume =
        !!run &&
        run.status === 'awaiting_approval' &&
        !!run.state &&
        all.every((a) => a.status !== 'pending');
      if (resume) await updateRun(tx, run.id, { status: 'running' });
      return { expired, decided, run, all, resume };
    });

    const { run, all, resume } = outcome;
    if (resume && run) {
      // Expired requests are reported to the agent as refusals.
      const decisions: Record<string, ApprovalDecision> = {};
      for (const a of all) {
        if (a.status === 'approved' || a.status === 'rejected' || a.status === 'expired') {
          decisions[a.id] = {
            approved: a.status === 'approved',
            ...(a.status === 'expired'
              ? { comment: 'demande de validation expirée' }
              : a.decisionComment
                ? { comment: a.decisionComment }
                : {}),
          };
        }
      }
      this.track(
        this.execute(access, run, {
          kind: 'resume',
          checkpoint: run.state as RunCheckpoint,
          decisions,
        }),
      );
    }
    if (outcome.expired) throw new DomainError('conflict', 'Cette demande de validation a expiré');
    return { approval: outcome.decided!, resumed: resume };
  }

  private autonomyLevel(access: AccessContext): AutonomyLevel {
    const configured = access.tenant.settings.defaultAutonomyLevel;
    return AUTONOMY_LEVELS.includes(configured as AutonomyLevel)
      ? (configured as AutonomyLevel)
      : 'execute_with_approval';
  }

  private track(promise: Promise<void>): void {
    this.inflight.add(promise);
    void promise.finally(() => this.inflight.delete(promise));
  }

  private async execute(
    access: AccessContext,
    run: RunRow,
    mode:
      | { kind: 'start'; history: LlmMessage[]; text: string }
      | { kind: 'resume'; checkpoint: RunCheckpoint; decisions: Record<string, ApprovalDecision> },
  ): Promise<void> {
    const { db, bus, logger } = this.deps;
    const tenantId = access.subject.tenantId;
    const userId = access.subject.userId;
    const ports = createDbPorts(db, { tenantId, runId: run.id, userId }, bus);

    try {
      const connectorTools = await this.deps.connectors.toolsFor(tenantId);
      const registry = new ToolRegistry(
        [
          ...createPlatformTools({
            engine: access.engine,
            orgTree: access.orgTree,
            memory: ports.memory,
            skills: ports.skills,
          }),
          ...connectorTools.tools,
        ],
        connectorTools.overrides,
      );
      const runtime = new AgentRuntime(
        {
          router: this.deps.router!,
          engine: access.engine,
          registry,
          ports,
          prices: this.deps.prices,
        },
        {
          runId: run.id,
          subject: access.subject,
          actor: { type: 'agent', runId: run.id, channel: 'web' },
          objective: run.objective,
          autonomy: { level: run.autonomyLevel as AutonomyLevel },
          budget: run.budget,
          successCriteria: run.successCriteria,
          verification: { llmJudge: this.deps.llmJudge, maxCorrectionAttempts: 2 },
          effort: 'high',
        },
      );

      let outcome: RunOutcome;
      if (mode.kind === 'start') {
        const system = await this.systemPrompt(access);
        const memories = await this.relevantMemories(access, mode.text);
        outcome = await runtime.start({
          system,
          history: mode.history,
          userTurn: buildUserTurn(mode.text, memories),
        });
      } else {
        outcome = await runtime.resume(mode.checkpoint, mode.decisions);
      }
      await this.persistOutcome(access, run, outcome);
    } catch (error) {
      logger.error({ err: error, runId: run.id }, 'Run crashed');
      await withTenant(db, tenantId, async (tx) => {
        await updateRun(tx, run.id, {
          status: 'failed',
          error: 'Erreur interne',
          finishedAt: new Date(),
        });
        if (run.conversationId) {
          await addMessage(tx, {
            tenantId,
            conversationId: run.conversationId,
            role: 'assistant',
            content:
              'Une erreur interne a interrompu le traitement. L’équipe technique a été informée.',
            runId: run.id,
            metadata: { status: 'failed' },
          });
        }
      }).catch((e: unknown) => logger.error({ err: e }, 'Failed to record run failure'));
      await ports.events
        .emit({ type: 'run_finished', status: 'failed', error: 'Erreur interne' })
        .catch(() => {});
    }
  }

  private async persistOutcome(access: AccessContext, run: RunRow, outcome: RunOutcome) {
    const tenantId = access.subject.tenantId;
    await withTenant(this.deps.db, tenantId, async (tx) => {
      const base = { state: outcome.checkpoint, usage: outcome.checkpoint.usage };
      if (outcome.status === 'awaiting_approval') {
        await updateRun(tx, run.id, { ...base, status: 'awaiting_approval' });
        return;
      }
      const finished = { ...base, finishedAt: new Date() };
      if (outcome.status === 'completed') {
        await updateRun(tx, run.id, {
          ...finished,
          status: 'completed',
          result: { answer: outcome.answer },
          verification: outcome.verification,
        });
      } else {
        await updateRun(tx, run.id, { ...finished, status: 'failed', error: outcome.error });
      }
      if (run.conversationId) {
        await addMessage(tx, {
          tenantId,
          conversationId: run.conversationId,
          role: 'assistant',
          content:
            outcome.status === 'completed'
              ? outcome.answer
              : `Je n’ai pas pu terminer cette demande : ${outcome.error}`,
          runId: run.id,
          metadata: {
            status: outcome.status,
            evidence: outcome.checkpoint.evidence.slice(0, 50),
            ...(outcome.status === 'completed' ? { verification: outcome.verification } : {}),
          },
        });
      }
      await appendAudit(tx, {
        tenantId,
        actorType: 'agent',
        onBehalfOfUserId: access.subject.userId,
        runId: run.id,
        action: `agent.run.${outcome.status}`,
        outcome: outcome.status === 'completed' ? 'success' : 'failure',
        metadata: { usage: outcome.checkpoint.usage },
      });
    });
  }

  private async systemPrompt(access: AccessContext): Promise<string> {
    const skills = await withTenant(this.deps.db, access.subject.tenantId, (tx) =>
      listActiveSkills(tx),
    );
    const primaryUnit = access.profile.primaryUnitId;
    return buildSystemPrompt(
      {
        assistantName: this.deps.assistantName,
        tenantName: access.tenant.name,
        displayName: access.profile.displayName,
        jobTitle: access.profile.jobTitle,
        orgUnitPath: primaryUnit ? access.orgTree.pathLabel(primaryUnit) : null,
        roles: access.grants,
      },
      skills,
      this.deps.now?.() ?? new Date(),
    );
  }

  private async relevantMemories(access: AccessContext, text: string) {
    const filter = memoryReadFilter(access.engine, access.orgTree, access.subject, {
      type: 'agent',
    });
    return withTenant(this.deps.db, access.subject.tenantId, async (tx) => {
      const hits = await searchMemory(tx, filter, text, {
        limit: 5,
        clearanceRank: sensitivityRank(access.subject.clearance ?? 'internal'),
      });
      await markMemoryUsed(
        tx,
        hits.map((h) => h.id),
      );
      return hits;
    });
  }
}

function toHistory(rows: Array<{ role: string; content: string }>): LlmMessage[] {
  return rows
    .filter((r) => r.content.trim().length > 0)
    .map((r) =>
      r.role === 'user'
        ? { role: 'user' as const, content: r.content }
        : { role: 'assistant' as const, content: r.content, toolCalls: [] },
    );
}
