/**
 * Opening a conversation turn: the user message, the run that will answer it
 * and the history replayed to the model, recorded in one transaction.
 */
import { DEFAULT_BUDGET, type LlmMessage } from '@mimorya/agent';
import { AUTONOMY_LEVELS, DomainError, NotFoundError, type AutonomyLevel } from '@mimorya/core';
import {
  addMessage,
  appendAudit,
  createRun,
  DEFAULT_CONVERSATION_TITLE,
  getConversation,
  listMessages,
  listRunsForConversation,
  updateConversation,
  type RunRow,
  type Transaction,
} from '@mimorya/db';
import type { AccessContext, AuthIdentity } from '../access';

const HISTORY_LIMIT = 20;
const TITLE_LENGTH = 80;

export interface OpenedTurn {
  run: RunRow;
  history: LlmMessage[];
  userMessageId: string;
}

export async function openConversationTurn(
  tx: Transaction,
  identity: AuthIdentity,
  conversationId: string,
  text: string,
  autonomy: AutonomyLevel,
): Promise<OpenedTurn> {
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
  if (conversation.title === DEFAULT_CONVERSATION_TITLE) {
    await updateConversation(tx, conversationId, { title: text.slice(0, TITLE_LENGTH) });
  }

  const run = await createRun(tx, {
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
    runId: run.id,
    action: 'agent.run.started',
    outcome: 'success',
    resourceType: 'conversation',
    resourceId: conversationId,
  });
  return { run, history: toHistory(previous), userMessageId: userMessage.id };
}

/** Autonomy level configured for the tenant, `execute_with_approval` by default. */
export function tenantAutonomyLevel(access: AccessContext): AutonomyLevel {
  const configured = access.tenant.settings.defaultAutonomyLevel;
  return AUTONOMY_LEVELS.includes(configured as AutonomyLevel)
    ? (configured as AutonomyLevel)
    : 'execute_with_approval';
}

/** Stored messages replayed as plain text turns (tool exchanges stay in the run). */
function toHistory(rows: Array<{ role: string; content: string }>): LlmMessage[] {
  return rows
    .filter((r) => r.content.trim().length > 0)
    .map((r) =>
      r.role === 'user'
        ? { role: 'user' as const, content: r.content }
        : { role: 'assistant' as const, content: r.content, toolCalls: [] },
    );
}
