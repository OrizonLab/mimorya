import { and, eq, isNull, sql } from 'drizzle-orm';
import type { Executor } from '../client';
import { sessions, users } from '../schema';

export interface LoginRecord {
  userId: string;
  tenantId: string;
  passwordHash: string | null;
  status: string;
  tenantStatus: string;
}

/** Pre-tenant lookup (SECURITY DEFINER function): the only cross-tenant read on login. */
export async function findLogin(db: Executor, email: string): Promise<LoginRecord | null> {
  const result = await db.execute<{
    user_id: string;
    tenant_id: string;
    password_hash: string | null;
    status: string;
    tenant_status: string;
  }>(sql`select * from auth_find_login(${email})`);
  const row = result.rows[0];
  if (!row) return null;
  return {
    userId: row.user_id,
    tenantId: row.tenant_id,
    passwordHash: row.password_hash,
    status: row.status,
    tenantStatus: row.tenant_status,
  };
}

export interface ResolvedSession {
  sessionId: string;
  userId: string;
  tenantId: string;
  expiresAt: Date;
  absoluteExpiresAt: Date;
}

/** Pre-tenant lookup (SECURITY DEFINER function): only valid, active sessions are returned. */
export async function resolveSession(
  db: Executor,
  tokenHash: string,
): Promise<ResolvedSession | null> {
  const result = await db.execute<{
    session_id: string;
    user_id: string;
    tenant_id: string;
    expires_at: string | Date;
    absolute_expires_at: string | Date;
  }>(sql`select * from auth_resolve_session(${tokenHash})`);
  const row = result.rows[0];
  if (!row) return null;
  return {
    sessionId: row.session_id,
    userId: row.user_id,
    tenantId: row.tenant_id,
    expiresAt: new Date(row.expires_at),
    absoluteExpiresAt: new Date(row.absolute_expires_at),
  };
}

export async function createSession(
  tx: Executor,
  input: {
    tenantId: string;
    userId: string;
    tokenHash: string;
    expiresAt: Date;
    absoluteExpiresAt: Date;
    ip?: string | null;
    userAgent?: string | null;
  },
): Promise<string> {
  const [row] = await tx
    .insert(sessions)
    .values({
      tenantId: input.tenantId,
      userId: input.userId,
      tokenHash: input.tokenHash,
      expiresAt: input.expiresAt,
      absoluteExpiresAt: input.absoluteExpiresAt,
      ip: input.ip ?? null,
      userAgent: input.userAgent?.slice(0, 500) ?? null,
    })
    .returning({ id: sessions.id });
  await tx.update(users).set({ lastLoginAt: new Date() }).where(eq(users.id, input.userId));
  return row!.id;
}

export async function touchSession(
  tx: Executor,
  sessionId: string,
  expiresAt: Date,
): Promise<void> {
  await tx
    .update(sessions)
    .set({ lastSeenAt: new Date(), expiresAt })
    .where(and(eq(sessions.id, sessionId), isNull(sessions.revokedAt)));
}

export async function revokeSession(tx: Executor, sessionId: string): Promise<void> {
  await tx.update(sessions).set({ revokedAt: new Date() }).where(eq(sessions.id, sessionId));
}
