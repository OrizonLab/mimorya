import { and, desc, eq, isNull } from 'drizzle-orm';
import type { Executor } from '../client';
import { conversations, messages } from '../schema';

export type ConversationRow = typeof conversations.$inferSelect;
export type MessageRow = typeof messages.$inferSelect;

export async function listConversations(
  tx: Executor,
  ownerUserId: string,
): Promise<ConversationRow[]> {
  return tx
    .select()
    .from(conversations)
    .where(and(eq(conversations.ownerUserId, ownerUserId), isNull(conversations.archivedAt)))
    .orderBy(desc(conversations.updatedAt))
    .limit(100);
}

export async function createConversation(
  tx: Executor,
  input: { tenantId: string; ownerUserId: string; title?: string },
): Promise<ConversationRow> {
  const [row] = await tx
    .insert(conversations)
    .values({
      tenantId: input.tenantId,
      ownerUserId: input.ownerUserId,
      ...(input.title ? { title: input.title } : {}),
    })
    .returning();
  return row!;
}

export async function getConversation(tx: Executor, id: string): Promise<ConversationRow | null> {
  const [row] = await tx.select().from(conversations).where(eq(conversations.id, id));
  return row ?? null;
}

export async function updateConversation(
  tx: Executor,
  id: string,
  patch: Partial<Pick<ConversationRow, 'title' | 'archivedAt'>>,
): Promise<void> {
  await tx
    .update(conversations)
    .set({ ...patch, updatedAt: new Date() })
    .where(eq(conversations.id, id));
}

export async function listMessages(
  tx: Executor,
  conversationId: string,
  limit = 200,
): Promise<MessageRow[]> {
  const rows = await tx
    .select()
    .from(messages)
    .where(eq(messages.conversationId, conversationId))
    .orderBy(desc(messages.createdAt))
    .limit(limit);
  return rows.reverse();
}

export async function addMessage(
  tx: Executor,
  input: {
    tenantId: string;
    conversationId: string;
    role: 'user' | 'assistant';
    content: string;
    authorUserId?: string | null;
    runId?: string | null;
    metadata?: Record<string, unknown>;
  },
): Promise<MessageRow> {
  const [row] = await tx
    .insert(messages)
    .values({
      tenantId: input.tenantId,
      conversationId: input.conversationId,
      role: input.role,
      content: input.content,
      authorUserId: input.authorUserId ?? null,
      runId: input.runId ?? null,
      metadata: input.metadata ?? {},
    })
    .returning();
  await tx
    .update(conversations)
    .set({ updatedAt: new Date() })
    .where(eq(conversations.id, input.conversationId));
  return row!;
}
