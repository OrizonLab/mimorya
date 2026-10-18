/**
 * Memory persistence and retrieval. Visibility is applied in SQL from the
 * filter computed by @mimorya/core (pre-filtering): rows outside the
 * subject's reach are never loaded, let alone ranked.
 */
import {
  SENSITIVITY_LEVELS,
  type MemoryKind,
  type MemoryReadFilter,
  type MemoryScope,
} from '@mimorya/core';
import { and, desc, eq, inArray, sql, type SQL } from 'drizzle-orm';
import type { Executor } from '../client';
import { memoryItems, type MemorySource } from '../schema';

export type MemoryRow = typeof memoryItems.$inferSelect;

const SENSITIVITY_ARRAY = sql.raw(`array[${SENSITIVITY_LEVELS.map((s) => `'${s}'`).join(',')}]`);

function visibility(filter: MemoryReadFilter, clearanceRank: number): SQL {
  const ownPersonal = sql`(${memoryItems.scope} = 'user' and ${memoryItems.ownerUserId} = ${filter.ownerUserId})`;
  if (!filter.shared) return ownPersonal;
  const cleared = sql`(array_position(${SENSITIVITY_ARRAY}, ${memoryItems.sensitivity}) - 1) <= ${clearanceRank}`;
  const units =
    filter.orgUnitIds === 'all'
      ? sql`${memoryItems.scope} = 'org_unit'`
      : filter.orgUnitIds.size > 0
        ? sql`(${memoryItems.scope} = 'org_unit' and ${inArray(memoryItems.orgUnitId, [...filter.orgUnitIds])})`
        : sql`false`;
  const companyWide = sql`${memoryItems.scope} in ('tenant', 'experience')`;
  return sql`(${ownPersonal} or ((${units} or ${companyWide}) and ${cleared}))`;
}

const active = sql`${memoryItems.status} = 'active' and (${memoryItems.expiresAt} is null or ${memoryItems.expiresAt} > now())`;

/** OR-query over the significant words of a natural-language request. */
export function toOrTsQuery(query: string): string | null {
  const words = query
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter((w) => w.length > 2)
    .slice(0, 24);
  return words.length > 0 ? [...new Set(words)].join(' | ') : null;
}

export interface MemorySearchHit {
  id: string;
  scope: MemoryScope;
  kind: MemoryKind;
  title: string;
  content: string;
  orgUnitId: string | null;
  updatedAt: Date;
  score: number;
}

export async function searchMemory(
  tx: Executor,
  filter: MemoryReadFilter,
  query: string,
  options: { limit: number; clearanceRank: number },
): Promise<MemorySearchHit[]> {
  const tsQuery = toOrTsQuery(query);
  if (!tsQuery) return [];
  const q = sql`to_tsquery('french', ${tsQuery})`;
  const rank = sql<number>`ts_rank_cd(${memoryItems.contentTsv}, ${q})`;
  const rows = await tx
    .select({
      id: memoryItems.id,
      scope: memoryItems.scope,
      kind: memoryItems.kind,
      title: memoryItems.title,
      content: memoryItems.content,
      orgUnitId: memoryItems.orgUnitId,
      updatedAt: memoryItems.updatedAt,
      score: rank,
    })
    .from(memoryItems)
    .where(
      and(
        active,
        visibility(filter, options.clearanceRank),
        sql`${memoryItems.contentTsv} @@ ${q}`,
      ),
    )
    .orderBy(desc(rank), desc(memoryItems.updatedAt))
    .limit(options.limit);
  return rows.map((r) => ({
    ...r,
    scope: r.scope as MemoryScope,
    kind: r.kind as MemoryKind,
    score: Number(r.score),
  }));
}

/** Memories visible to the subject, newest first (memory management screen). */
export async function listVisibleMemory(
  tx: Executor,
  filter: MemoryReadFilter,
  clearanceRank: number,
  options: { includeProposed?: boolean; limit?: number } = {},
): Promise<MemoryRow[]> {
  const status = options.includeProposed
    ? sql`${memoryItems.status} in ('active', 'proposed')`
    : sql`${memoryItems.status} = 'active'`;
  return tx
    .select()
    .from(memoryItems)
    .where(and(status, visibility(filter, clearanceRank)))
    .orderBy(desc(memoryItems.updatedAt))
    .limit(options.limit ?? 200);
}

export async function createMemory(
  tx: Executor,
  input: {
    tenantId: string;
    scope: MemoryScope;
    ownerUserId?: string | null;
    orgUnitId?: string | null;
    kind: MemoryKind;
    title: string;
    content: string;
    sensitivity?: MemoryRow['sensitivity'];
    status: 'active' | 'proposed';
    source: MemorySource;
    createdByUserId?: string | null;
    createdByRunId?: string | null;
  },
): Promise<{ id: string }> {
  const [row] = await tx
    .insert(memoryItems)
    .values({
      tenantId: input.tenantId,
      scope: input.scope,
      ownerUserId: input.ownerUserId ?? null,
      orgUnitId: input.orgUnitId ?? null,
      kind: input.kind,
      title: input.title,
      content: input.content,
      sensitivity: input.sensitivity ?? 'internal',
      status: input.status,
      source: input.source,
      createdByUserId: input.createdByUserId ?? null,
      createdByRunId: input.createdByRunId ?? null,
    })
    .returning({ id: memoryItems.id });
  return row!;
}

export async function getMemory(tx: Executor, id: string): Promise<MemoryRow | null> {
  const [row] = await tx.select().from(memoryItems).where(eq(memoryItems.id, id));
  return row ?? null;
}

export async function markMemoryUsed(tx: Executor, ids: string[]): Promise<void> {
  if (ids.length === 0) return;
  await tx
    .update(memoryItems)
    .set({ lastUsedAt: new Date(), useCount: sql`${memoryItems.useCount} + 1` })
    .where(inArray(memoryItems.id, ids));
}

export async function setMemoryStatus(
  tx: Executor,
  id: string,
  status: MemoryRow['status'],
): Promise<void> {
  await tx.update(memoryItems).set({ status }).where(eq(memoryItems.id, id));
}

/** Hard delete (right to erasure); the audit log keeps the trace without the content. */
export async function deleteMemory(tx: Executor, id: string): Promise<void> {
  await tx.delete(memoryItems).where(eq(memoryItems.id, id));
}
