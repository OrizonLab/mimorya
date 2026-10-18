/**
 * Instantiates the connectors enabled for a tenant and keeps them alive
 * across requests (connector state, MCP sessions). An instance is rebuilt
 * when its configuration changes.
 */
import type { ToolOverride } from '@mimorya/agent';
import { CONNECTOR_TYPES, type ConnectorRuntime } from '@mimorya/connectors';
import type { AnyToolDefinition } from '@mimorya/core';
import { listConnectorInstances, withTenant, type Database } from '@mimorya/db';
import type { FastifyBaseLogger } from 'fastify';

interface CachedRuntime {
  version: string;
  runtime: Promise<ConnectorRuntime>;
}

export interface TenantTools {
  tools: AnyToolDefinition[];
  overrides: Record<string, ToolOverride>;
  errors: Array<{ connector: string; message: string }>;
}

export class ConnectorService {
  private readonly runtimes = new Map<string, CachedRuntime>();

  constructor(
    private readonly db: Database,
    private readonly logger: FastifyBaseLogger,
  ) {}

  async toolsFor(tenantId: string): Promise<TenantTools> {
    const instances = await withTenant(this.db, tenantId, (tx) => listConnectorInstances(tx, true));
    const result: TenantTools = { tools: [], overrides: {}, errors: [] };
    for (const instance of instances) {
      const definition = CONNECTOR_TYPES.get(instance.type);
      if (!definition) {
        result.errors.push({
          connector: instance.name,
          message: `Type inconnu : ${instance.type}`,
        });
        continue;
      }
      const version = instance.updatedAt.toISOString();
      let cached = this.runtimes.get(instance.id);
      if (!cached || cached.version !== version) {
        if (cached) void cached.runtime.then((r) => r.close?.()).catch(() => undefined);
        // Secrets are not wired yet: connectors needing credentials arrive with
        // the encrypted secret store (see ROADMAP).
        cached = {
          version,
          runtime: Promise.resolve(definition.instantiate(instance.config as never)),
        };
        this.runtimes.set(instance.id, cached);
      }
      try {
        const runtime = await cached.runtime;
        result.tools.push(...runtime.tools);
        Object.assign(result.overrides, instance.toolPolicies);
      } catch (error) {
        this.runtimes.delete(instance.id);
        this.logger.warn({ err: error, connector: instance.id }, 'Connector unavailable');
        result.errors.push({ connector: instance.name, message: 'Connecteur indisponible' });
      }
    }
    return result;
  }

  async close(): Promise<void> {
    await Promise.allSettled(
      [...this.runtimes.values()].map(async (c) => (await c.runtime).close?.()),
    );
    this.runtimes.clear();
  }
}
