import { demoInterventionsConnector } from './demo-interventions/connector';
import { mcpConnector } from './mcp/bridge';
import type { ConnectorDefinition } from './types';

/** Connector types available on this platform build. */
export const CONNECTOR_TYPES: ReadonlyMap<string, ConnectorDefinition<never>> = new Map(
  [demoInterventionsConnector, mcpConnector].map((c) => [c.type, c as ConnectorDefinition<never>]),
);
