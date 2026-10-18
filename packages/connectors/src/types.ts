import type { AnyToolDefinition, PermissionInfo } from '@mimorya/core';

/** A live connector instance: its tools and how to release its resources. */
export interface ConnectorRuntime {
  tools: AnyToolDefinition[];
  close?(): Promise<void>;
}

/**
 * A connector type. Instances are configured per tenant (`connector_instances`)
 * and created once per process, then reused across requests.
 */
export interface ConnectorDefinition<Config = Record<string, unknown>> {
  type: string;
  name: string;
  description: string;
  /** Demo connectors serve fictional data and are labelled as such in the UI. */
  demo?: boolean;
  /** Business permissions introduced by this connector. */
  permissions: readonly PermissionInfo[];
  instantiate(config: Config, secret?: string): ConnectorRuntime | Promise<ConnectorRuntime>;
}
