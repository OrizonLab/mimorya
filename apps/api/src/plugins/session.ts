/**
 * Session resolution: the session cookie is resolved on every request into
 * `request.auth`, and the idle expiry is extended at most once per interval.
 */
import { DomainError } from '@mimorya/core';
import { resolveSession, touchSession, withTenant, type Database } from '@mimorya/db';
import type { FastifyInstance, FastifyRequest } from 'fastify';
import {
  hashToken,
  SESSION_COOKIE,
  SESSION_IDLE_MS,
  SESSION_TOUCH_INTERVAL_MS,
} from '../auth/credentials';
import type { AuthIdentity } from '../services/access';

declare module 'fastify' {
  interface FastifyRequest {
    auth: AuthIdentity | null;
  }
}

/** Identity of the signed-in user; every route starts with this call. */
export function requireAuth(request: FastifyRequest): AuthIdentity {
  if (!request.auth) throw new DomainError('unauthenticated', 'Authentification requise');
  return request.auth;
}

export function registerSessionResolver(app: FastifyInstance, db: Database): void {
  app.decorateRequest('auth', null);
  app.addHook('onRequest', async (request) => {
    const token = request.cookies[SESSION_COOKIE];
    if (!token) return;
    const session = await resolveSession(db, hashToken(token));
    if (!session) return;
    request.auth = {
      tenantId: session.tenantId,
      userId: session.userId,
      sessionId: session.sessionId,
    };
    const remaining = session.expiresAt.getTime() - Date.now();
    if (SESSION_IDLE_MS - remaining > SESSION_TOUCH_INTERVAL_MS) {
      const expiresAt = new Date(
        Math.min(Date.now() + SESSION_IDLE_MS, session.absoluteExpiresAt.getTime()),
      );
      await withTenant(db, session.tenantId, (tx) =>
        touchSession(tx, session.sessionId, expiresAt),
      );
    }
  });
}
