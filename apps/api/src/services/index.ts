/** Application services, built once per server and shared by the routes. */
import { createProvider, ModelRouter, PriceTable, type LlmProvider } from '@mimorya/agent';
import type { Database } from '@mimorya/db';
import type { FastifyBaseLogger } from 'fastify';
import type { AppConfig } from '../config';
import { AccessService } from './access';
import { ConnectorService } from './connectors';
import { RunEventBus } from './event-bus';
import { RunService } from './runs';

export interface AppServices {
  config: AppConfig;
  db: Database;
  access: AccessService;
  runs: RunService;
  connectors: ConnectorService;
  bus: RunEventBus;
}

export interface ServiceOptions {
  /** Injected model provider (tests); otherwise built from configuration. */
  provider?: LlmProvider | null;
  now?: () => Date;
}

export function createServices(
  config: AppConfig,
  db: Database,
  logger: FastifyBaseLogger,
  options: ServiceOptions = {},
): AppServices {
  const provider =
    options.provider !== undefined
      ? options.provider
      : config.llm
        ? createProvider(config.llm)
        : null;
  const bus = new RunEventBus();
  const access = new AccessService(db);
  const connectors = new ConnectorService(db, logger);
  const runs = new RunService({
    db,
    router: provider ? new ModelRouter({ agent: provider }) : null,
    prices: new PriceTable(),
    access,
    connectors,
    bus,
    logger,
    assistantName: config.assistantName,
    llmJudge: config.llmJudge,
    ...(options.now ? { now: options.now } : {}),
  });
  return { config, db, access, runs, connectors, bus };
}
