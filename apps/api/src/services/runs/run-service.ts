/**
 * Entry point for agent runs started from conversations: opens a turn,
 * records human decisions on approvals, and hands runs to the executor in
 * the background.
 */
import type { RunCheckpoint } from '@mimorya/agent';
import { DomainError } from '@mimorya/core';
import { withTenant, type ApprovalRow } from '@mimorya/db';
import type { AccessService, AuthIdentity } from '../access';
import { recordApprovalDecision, resumeDecisions, type DecisionInput } from './approval-decision';
import { openConversationTurn, tenantAutonomyLevel } from './conversation-turn';
import { RunExecutor, type RunExecutorDeps } from './run-executor';

export interface RunServiceDeps extends RunExecutorDeps {
  access: AccessService;
}

export class RunService {
  private readonly inflight = new Set<Promise<void>>();
  private readonly executor: RunExecutor;

  constructor(private readonly deps: RunServiceDeps) {
    this.executor = new RunExecutor(deps);
  }

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
    const autonomy = tenantAutonomyLevel(access);

    const { run, history, userMessageId } = await withTenant(
      this.deps.db,
      identity.tenantId,
      (tx) => openConversationTurn(tx, identity, conversationId, text, autonomy),
    );
    this.track(this.executor.execute(access, run, { kind: 'start', history, text }));
    return { runId: run.id, messageId: userMessageId };
  }

  async decide(
    identity: AuthIdentity,
    input: DecisionInput,
  ): Promise<{ approval: ApprovalRow; resumed: boolean }> {
    const access = await this.deps.access.context(identity);
    const recorded = await withTenant(this.deps.db, identity.tenantId, (tx) =>
      recordApprovalDecision(tx, identity, input, new Date()),
    );

    if (recorded.resume) {
      const { run, approvals } = recorded.resume;
      this.track(
        this.executor.execute(access, run, {
          kind: 'resume',
          checkpoint: run.state as RunCheckpoint,
          decisions: resumeDecisions(approvals),
        }),
      );
    }
    if (recorded.expired || !recorded.decided) {
      throw new DomainError('conflict', 'Cette demande de validation a expiré');
    }
    return { approval: recorded.decided, resumed: recorded.resume !== null };
  }

  private track(promise: Promise<void>): void {
    this.inflight.add(promise);
    void promise.finally(() => this.inflight.delete(promise));
  }
}
