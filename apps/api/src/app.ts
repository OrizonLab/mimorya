import cookie from '@fastify/cookie';
import rateLimit from '@fastify/rate-limit';
import { createProvider, ModelRouter, PriceTable, type LlmProvider } from '@mimorya/agent';
import { DomainError } from '@mimorya/core';
import {
  createDatabase,
  failStaleRuns,
  resolveSession,
  touchSession,
  withTenant,
} from '@mimorya/db';
import Fastify, { type FastifyInstance, type FastifyRequest } from 'fastify';
import { z } from 'zod';
import {
  hashToken,
  SESSION_COOKIE,
  SESSION_IDLE_MS,
  SESSION_TOUCH_INTERVAL_MS,
} from './auth/credentials';
import type { AppConfig } from './config';
import { errorHandler } from './http';
import { registerAdminRoutes } from './routes/admin';
import { registerAuthRoutes } from './routes/auth';
import { registerConversationRoutes } from './routes/conversations';
import { registerMemoryRoutes } from './routes/memory';
import { registerRunRoutes } from './routes/runs';
import { AccessService, type AuthIdentity } from './services/access';
import { ConnectorService } from './services/connectors';
import { RunEventBus } from './services/event-bus';
import { RunService } from './services/runs';

declare module 'fastify' {
  interface FastifyRequest {
    auth: AuthIdentity | null;
  }
}

export interface AppServices {
  config: AppConfig;
  db: ReturnType<typeof createDatabase>['db'];
  access: AccessService;
  runs: RunService;
  connectors: ConnectorService;
  bus: RunEventBus;
}

export function requireAuth(request: FastifyRequest): AuthIdentity {
  if (!request.auth) throw new DomainError('unauthenticated', 'Authentification requise');
  return request.auth;
}

const SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS']);

export interface BuildOptions {
  /** Injected model provider (tests); otherwise built from configuration. */
  provider?: LlmProvider | null;
  now?: () => Date;
}

export async function buildApp(
  config: AppConfig,
  options: BuildOptions = {},
): Promise<FastifyInstance> {
  // French validation messages for users.
  z.config(z.locales.fr());

  const app = Fastify({
    logger: {
      level: config.logLevel,
      redact: ['req.headers.cookie', 'req.headers.authorization', 'res.headers["set-cookie"]'],
    },
    trustProxy: true,
    bodyLimit: 1_000_000,
  });

  const database = createDatabase(config.databaseAppUrl, { applicationName: 'mimorya-api' });
  const provider =
    options.provider !== undefined
      ? options.provider
      : config.llm
        ? createProvider(config.llm)
        : null;
  const bus = new RunEventBus();
  const access = new AccessService(database.db);
  const connectors = new ConnectorService(database.db, app.log);
  const runs = new RunService({
    db: database.db,
    router: provider ? new ModelRouter({ agent: provider }) : null,
    prices: new PriceTable(),
    access,
    connectors,
    bus,
    logger: app.log,
    assistantName: config.assistantName,
    llmJudge: config.llmJudge,
    ...(options.now ? { now: options.now } : {}),
  });
  const services: AppServices = { config, db: database.db, access, runs, connectors, bus };

  app.setErrorHandler(errorHandler);
  await app.register(cookie);
  await app.register(rateLimit, { max: 600, timeWindow: '1 minute' });

  // CSRF protection: state-changing requests must carry a custom header (not
  // settable cross-site without CORS, which is disabled) and, when present,
  // an Origin matching the web application.
  app.addHook('onRequest', async (request) => {
    if (SAFE_METHODS.has(request.method)) return;
    if (request.headers['x-mimorya-request'] !== '1') {
      throw new DomainError('forbidden', 'En-tête de requête manquant');
    }
    const origin = request.headers.origin;
    if (origin && origin !== config.webOrigin) {
      throw new DomainError('forbidden', 'Origine non autorisée');
    }
  });

  app.decorateRequest('auth', null);
  app.addHook('onRequest', async (request) => {
    const token = request.cookies[SESSION_COOKIE];
    if (!token) return;
    const session = await resolveSession(database.db, hashToken(token));
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
      await withTenant(database.db, session.tenantId, (tx) =>
        touchSession(tx, session.sessionId, expiresAt),
      );
    }
  });

  app.get('/health', async () => {
    await database.pool.query('select 1');
    return { status: 'ok', llm: runs.llmAvailable };
  });

  registerAuthRoutes(app, services);
  registerConversationRoutes(app, services);
  registerRunRoutes(app, services);
  registerMemoryRoutes(app, services);
  registerAdminRoutes(app, services);

  // Runs without progress for 15 minutes belong to a crashed or stuck process.
  const sweep = async () => {
    const failed = await failStaleRuns(database.db, new Date(Date.now() - 15 * 60_000)).catch(
      (err: unknown) => {
        app.log.warn({ err }, 'Stale run sweep failed');
        return 0;
      },
    );
    if (failed > 0) app.log.warn({ failed }, 'Stale runs marked as failed');
  };
  const sweeper = setInterval(() => void sweep(), 60_000);
  sweeper.unref();
  app.addHook('onReady', sweep);

  app.addHook('onClose', async () => {
    clearInterval(sweeper);
    await runs.idle();
    await connectors.close();
    await database.close();
  });

  app.decorate('services', services);
  return app;
}

declare module 'fastify' {
  interface FastifyInstance {
    services: AppServices;
  }
}
