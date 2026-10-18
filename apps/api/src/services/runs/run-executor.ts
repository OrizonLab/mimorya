/**
 * Executes one run in the background: assembles the agent runtime for the
 * user, starts or resumes it, and persists the outcome. A crash is recorded
 * as a failed run; it never propagates to the caller.
 */
import {
  AgentRuntime,
  buildUserTurn,
  type ApprovalDecision,
  type EnginePorts,
  type LlmMessage,
  type ModelRouter,
  type PriceTable,
  type RunCheckpoint,
} from '@mimorya/agent';
import type { AutonomyLevel } from '@mimorya/core';
import type { Database, RunRow } from '@mimorya/db';
import type { FastifyBaseLogger } from 'fastify';
import type { AccessContext } from '../access';
import type { ConnectorService } from '../connectors';
import type { RunEventBus } from '../event-bus';
import { createDbPorts } from './db-ports';
import { buildRunSystemPrompt, buildToolRegistry, recallMemories } from './run-context';
import { recordCrash } from './run-outcome';

export interface RunExecutorDeps {
  db: Database;
  router: ModelRouter | null;
  prices: PriceTable;
  connectors: ConnectorService;
  bus: RunEventBus;
  logger: FastifyBaseLogger;
  assistantName: string;
  llmJudge: boolean;
  now?: () => Date;
}

export type ExecutionMode =
  | { kind: 'start'; history: LlmMessage[]; text: string }
  | { kind: 'resume'; checkpoint: RunCheckpoint; decisions: Record<string, ApprovalDecision> };

const MAX_CORRECTION_ATTEMPTS = 2;

export class RunExecutor {
  constructor(private readonly deps: RunExecutorDeps) {}

  async execute(access: AccessContext, run: RunRow, mode: ExecutionMode): Promise<void> {
    const { db, bus, logger } = this.deps;
    const tenantId = access.subject.tenantId;
    const scope = {
      tenantId,
      runId: run.id,
      userId: access.subject.userId,
      conversationId: run.conversationId,
    };
    const ports = createDbPorts(db, scope, bus);

    try {
      const runtime = await this.runtimeFor(access, run, ports);
      // The runtime records the outcome through its ports before announcing it.
      if (mode.kind === 'start') {
        const system = await buildRunSystemPrompt(
          db,
          access,
          this.deps.assistantName,
          this.deps.now?.() ?? new Date(),
        );
        const memories = await recallMemories(db, access, mode.text);
        await runtime.start({
          system,
          history: mode.history,
          userTurn: buildUserTurn(mode.text, memories),
        });
      } else {
        await runtime.resume(mode.checkpoint, mode.decisions);
      }
    } catch (error) {
      logger.error({ err: error, runId: run.id }, 'Run crashed');
      await recordCrash(db, scope).catch((e: unknown) =>
        logger.error({ err: e }, 'Failed to record run failure'),
      );
      await ports.events
        .emit({ type: 'run_finished', status: 'failed', error: 'Erreur interne' })
        .catch(() => {});
    }
  }

  private async runtimeFor(
    access: AccessContext,
    run: RunRow,
    ports: EnginePorts,
  ): Promise<AgentRuntime> {
    // A run awaiting approval may resume after a restart without a provider.
    if (!this.deps.router) throw new Error('No model provider configured');
    const registry = await buildToolRegistry(access, ports, this.deps.connectors);
    return new AgentRuntime(
      {
        router: this.deps.router,
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
        verification: {
          llmJudge: this.deps.llmJudge,
          maxCorrectionAttempts: MAX_CORRECTION_ATTEMPTS,
        },
        effort: 'high',
      },
    );
  }
}
