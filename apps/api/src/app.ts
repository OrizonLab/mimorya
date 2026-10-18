/**
 * Composition root of the API: services, cross-cutting hooks, routes and
 * lifecycle. No business rule lives here.
 */
import cookie from '@fastify/cookie';
import rateLimit from '@fastify/rate-limit';
import { createDatabase } from '@mimorya/db';
import Fastify, { type FastifyInstance } from 'fastify';
import { z } from 'zod';
import type { AppConfig } from './config';
import { errorHandler } from './http';
import { registerCsrfProtection } from './plugins/csrf';
import { registerSessionResolver } from './plugins/session';
import { registerStaleRunSweeper } from './plugins/stale-run-sweeper';
import { registerRoutes } from './routes';
import { createServices, type AppServices, type ServiceOptions } from './services';

export type BuildOptions = ServiceOptions;

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
  const services = createServices(config, database.db, app.log, options);

  app.setErrorHandler(errorHandler);
  await app.register(cookie);
  await app.register(rateLimit, { max: 600, timeWindow: '1 minute' });
  registerCsrfProtection(app, config.webOrigin);
  registerSessionResolver(app, database.db);

  app.get('/health', async () => {
    await database.pool.query('select 1');
    return { status: 'ok', llm: services.runs.llmAvailable };
  });
  registerRoutes(app, services);
  registerStaleRunSweeper(app, database.db);

  app.addHook('onClose', async () => {
    await services.runs.idle();
    await services.connectors.close();
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
