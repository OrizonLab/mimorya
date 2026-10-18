import { CONNECTOR_TYPES } from '@mimorya/connectors';
import { DomainError, PLATFORM_PERMISSIONS, type Permission } from '@mimorya/core';
import { appendAudit, createSession, findLogin, revokeSession, withTenant } from '@mimorya/db';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import { requireAuth, type AppServices } from '../app';
import {
  newSessionToken,
  SESSION_ABSOLUTE_MS,
  SESSION_COOKIE,
  SESSION_IDLE_MS,
  verifyPassword,
} from '../auth/credentials';
import { parse } from '../http';

const loginSchema = z.object({
  email: z.string().trim().toLowerCase().email().max(254),
  password: z.string().min(1).max(512),
});

const INVALID = 'Identifiants invalides';

export function registerAuthRoutes(app: FastifyInstance, services: AppServices) {
  const { db, config, access, runs } = services;

  app.post(
    '/auth/login',
    { config: { rateLimit: { max: 10, timeWindow: '1 minute' } } },
    async (request, reply) => {
      const body = parse(loginSchema, request.body);
      const login = await findLogin(db, body.email);
      const valid = await verifyPassword(login?.passwordHash ?? null, body.password);
      if (!login || !valid || login.status !== 'active' || login.tenantStatus !== 'active') {
        if (login) {
          await withTenant(db, login.tenantId, (tx) =>
            appendAudit(tx, {
              tenantId: login.tenantId,
              actorType: 'user',
              actorUserId: login.userId,
              action: 'auth.login',
              outcome: 'denied',
              reason: valid ? 'Compte ou entreprise inactif' : 'Mot de passe invalide',
              ip: request.ip,
              requestId: request.id,
            }),
          );
        }
        throw new DomainError('unauthenticated', INVALID);
      }

      const { token, tokenHash } = newSessionToken();
      const now = Date.now();
      await withTenant(db, login.tenantId, async (tx) => {
        await createSession(tx, {
          tenantId: login.tenantId,
          userId: login.userId,
          tokenHash,
          expiresAt: new Date(now + SESSION_IDLE_MS),
          absoluteExpiresAt: new Date(now + SESSION_ABSOLUTE_MS),
          ip: request.ip,
          userAgent: request.headers['user-agent'] ?? null,
        });
        await appendAudit(tx, {
          tenantId: login.tenantId,
          actorType: 'user',
          actorUserId: login.userId,
          action: 'auth.login',
          outcome: 'success',
          ip: request.ip,
          requestId: request.id,
        });
      });
      reply.setCookie(SESSION_COOKIE, token, {
        httpOnly: true,
        secure: config.secureCookies,
        sameSite: 'lax',
        path: '/',
        maxAge: SESSION_ABSOLUTE_MS / 1000,
      });
      return { ok: true };
    },
  );

  app.post('/auth/logout', async (request, reply) => {
    const identity = request.auth;
    if (identity) {
      await withTenant(db, identity.tenantId, async (tx) => {
        await revokeSession(tx, identity.sessionId);
        await appendAudit(tx, {
          tenantId: identity.tenantId,
          actorType: 'user',
          actorUserId: identity.userId,
          action: 'auth.logout',
          outcome: 'success',
          ip: request.ip,
          requestId: request.id,
        });
      });
    }
    reply.clearCookie(SESSION_COOKIE, { path: '/' });
    return { ok: true };
  });

  app.get('/auth/me', async (request) => {
    const ctx = await access.context(requireAuth(request));
    const catalog: Permission[] = [
      ...PLATFORM_PERMISSIONS.map((p) => p.permission),
      ...[...CONNECTOR_TYPES.values()].flatMap((c) => c.permissions.map((p) => p.permission)),
    ];
    const primaryUnit = ctx.profile.primaryUnitId;
    return {
      user: {
        id: ctx.profile.id,
        email: ctx.profile.email,
        displayName: ctx.profile.displayName,
        jobTitle: ctx.profile.jobTitle,
        orgUnit: primaryUnit ? { id: primaryUnit, path: ctx.orgTree.pathLabel(primaryUnit) } : null,
      },
      tenant: { id: ctx.tenant.id, name: ctx.tenant.name, demo: ctx.tenant.settings.demo === true },
      grants: ctx.grants,
      permissions: [...new Set(catalog)].filter((p) => ctx.engine.has(ctx.subject, p)).sort(),
      assistant: { name: config.assistantName, available: runs.llmAvailable },
    };
  });
}
