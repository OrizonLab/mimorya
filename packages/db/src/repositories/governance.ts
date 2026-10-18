/** Audit trail, usage metering, skills and connector configuration. */
import { and, desc, eq, gte, sql } from 'drizzle-orm';
import type { Executor } from '../client';
import { auditLog, connectorInstances, skills, skillVersions, usageRecords } from '../schema';

export type AuditRow = typeof auditLog.$inferSelect;

export async function appendAudit(
  tx: Executor,
  entry: Omit<typeof auditLog.$inferInsert, 'id' | 'occurredAt'>,
): Promise<void> {
  await tx.insert(auditLog).values(entry);
}

export async function listAudit(
  tx: Executor,
  options: { limit?: number; beforeId?: number; runId?: string } = {},
): Promise<AuditRow[]> {
  const conditions = [
    options.beforeId ? sql`${auditLog.id} < ${options.beforeId}` : undefined,
    options.runId ? eq(auditLog.runId, options.runId) : undefined,
  ].filter((c) => c !== undefined);
  return tx
    .select()
    .from(auditLog)
    .where(conditions.length ? and(...conditions) : undefined)
    .orderBy(desc(auditLog.id))
    .limit(Math.min(options.limit ?? 100, 500));
}

export async function appendUsage(
  tx: Executor,
  entry: Omit<typeof usageRecords.$inferInsert, 'id' | 'createdAt'>,
): Promise<void> {
  await tx.insert(usageRecords).values(entry);
}

export interface UsageSummaryRow {
  provider: string;
  model: string;
  kind: string;
  calls: number;
  inputTokens: number;
  outputTokens: number;
  costMicros: number;
}

export async function usageSummary(tx: Executor, since: Date): Promise<UsageSummaryRow[]> {
  const rows = await tx
    .select({
      provider: usageRecords.provider,
      model: usageRecords.model,
      kind: usageRecords.kind,
      calls: sql<number>`count(*)::int`,
      inputTokens: sql<number>`coalesce(sum(${usageRecords.inputTokens}), 0)::bigint`,
      outputTokens: sql<number>`coalesce(sum(${usageRecords.outputTokens}), 0)::bigint`,
      costMicros: sql<number>`coalesce(sum(${usageRecords.costMicros}), 0)::bigint`,
    })
    .from(usageRecords)
    .where(gte(usageRecords.createdAt, since))
    .groupBy(usageRecords.provider, usageRecords.model, usageRecords.kind);
  return rows.map((r) => ({
    ...r,
    calls: Number(r.calls),
    inputTokens: Number(r.inputTokens),
    outputTokens: Number(r.outputTokens),
    costMicros: Number(r.costMicros),
  }));
}

export async function listActiveSkills(tx: Executor) {
  return tx
    .select({ name: skills.name, description: skills.description })
    .from(skills)
    .where(eq(skills.status, 'active'))
    .orderBy(skills.name);
}

export async function getActiveSkill(tx: Executor, name: string) {
  const [row] = await tx
    .select({ name: skills.name, version: skillVersions.version, content: skillVersions.content })
    .from(skills)
    .innerJoin(skillVersions, eq(skillVersions.id, skills.activeVersionId))
    .where(and(eq(skills.name, name), eq(skills.status, 'active')));
  return row ?? null;
}

export async function listSkills(tx: Executor) {
  return tx
    .select({
      id: skills.id,
      name: skills.name,
      description: skills.description,
      status: skills.status,
      activeVersion: skillVersions.version,
      updatedAt: skills.updatedAt,
    })
    .from(skills)
    .leftJoin(skillVersions, eq(skillVersions.id, skills.activeVersionId))
    .orderBy(skills.name);
}

export async function listConnectorInstances(tx: Executor, onlyEnabled = false) {
  return tx
    .select()
    .from(connectorInstances)
    .where(onlyEnabled ? eq(connectorInstances.enabled, true) : undefined)
    .orderBy(desc(connectorInstances.createdAt));
}
