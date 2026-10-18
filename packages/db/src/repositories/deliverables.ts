/** Deliverables produced by runs (draft plannings, reports…). */
import { desc, eq } from 'drizzle-orm';
import type { Executor } from '../client';
import { deliverables } from '../schema';

export type DeliverableRow = typeof deliverables.$inferSelect;

export async function createDeliverable(
  tx: Executor,
  input: Omit<typeof deliverables.$inferInsert, 'id' | 'createdAt' | 'updatedAt' | 'status'>,
): Promise<DeliverableRow> {
  const [row] = await tx.insert(deliverables).values(input).returning();
  return row!;
}

export async function getDeliverable(tx: Executor, id: string): Promise<DeliverableRow | null> {
  if (!/^[0-9a-f-]{36}$/i.test(id)) return null;
  const [row] = await tx.select().from(deliverables).where(eq(deliverables.id, id));
  return row ?? null;
}

export async function setDeliverableStatus(
  tx: Executor,
  id: string,
  status: DeliverableRow['status'],
): Promise<void> {
  await tx.update(deliverables).set({ status }).where(eq(deliverables.id, id));
}

export async function listDeliverables(
  tx: Executor,
  ownerUserId: string,
  limit = 50,
): Promise<DeliverableRow[]> {
  return tx
    .select()
    .from(deliverables)
    .where(eq(deliverables.ownerUserId, ownerUserId))
    .orderBy(desc(deliverables.createdAt))
    .limit(limit);
}
