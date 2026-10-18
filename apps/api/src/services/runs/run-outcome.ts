/**
 * Persisting what a run produced: its status and usage, the assistant message
 * shown in the conversation, and the audit entry.
 */
import type { RunOutcome } from '@mimorya/agent';
import {
  addMessage,
  appendAudit,
  updateRun,
  withTenant,
  type Database,
  type RunRow,
} from '@mimorya/db';
import type { AccessContext } from '../access';

const MAX_EVIDENCE_IN_MESSAGE = 50;

export async function persistOutcome(
  db: Database,
  access: AccessContext,
  run: RunRow,
  outcome: RunOutcome,
): Promise<void> {
  const tenantId = access.subject.tenantId;
  await withTenant(db, tenantId, async (tx) => {
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
          evidence: outcome.checkpoint.evidence.slice(0, MAX_EVIDENCE_IN_MESSAGE),
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

/** Marks a run that crashed as failed, without exposing the internal error. */
export async function recordCrash(db: Database, tenantId: string, run: RunRow): Promise<void> {
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
  });
}
