/** Connectors as `/admin/connectors` describes them. */

export interface ToolPolicy {
  enabled?: boolean | null;
  risk?: string | null;
  permissions?: string[] | null;
}

export interface ConnectorView {
  id: string;
  type: string;
  name: string;
  enabled: boolean;
  status: 'ok' | 'error' | 'disabled';
  lastError: string | null;
  toolPolicies: Record<string, ToolPolicy>;
  /** An MCP server configured over HTTPS (editable here). */
  mcp: { prefix: string; url: string; tokenStored: boolean } | null;
  updatedAt: string;
}

export interface ConnectorsView {
  instances: ConnectorView[];
  types: Array<{ type: string; name: string; description: string; demo: boolean }>;
  /** What a tool may require: permissions an agent can hold. */
  permissions: Array<{ permission: string; label: string }>;
  risks: string[];
  secretsAvailable: boolean;
  /** The operator lets connectors reach its private network (private deployment). */
  allowPrivateNetwork: boolean;
}

export interface TestedTool {
  name: string;
  title: string;
  description: string;
  enabled: boolean;
  risk: string;
  permissions: string[];
}

export interface ConnectorTest {
  ok: boolean;
  tools: TestedTool[];
  error: string | null;
}
