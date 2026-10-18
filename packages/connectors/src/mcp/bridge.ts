/**
 * MCP bridge: exposes the tools of an MCP server as platform tools.
 *
 * MCP servers know nothing about our users, so the bridge is conservative:
 * every MCP tool defaults to risk `write` (human validation required) and to
 * the `connector:use` permission, until an administrator maps it explicitly
 * (connector `tool_policies`). Results are untrusted data like any tool output.
 */
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js';
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js';
import type { Transport } from '@modelcontextprotocol/sdk/shared/transport.js';
import { defineTool, type AnyToolDefinition, type Permission, type ToolRisk } from '@mimorya/core';
import { z } from 'zod';
import type { ConnectorDefinition } from '../types';

export interface McpToolsetOptions {
  /** Short slug used to namespace tool names, e.g. "docs" → "mcp_docs_search". */
  prefix: string;
  defaultRisk?: ToolRisk;
  defaultPermissions?: Permission[];
  timeoutMs?: number;
}

export function mcpToolName(prefix: string, tool: string): string {
  const slug = (s: string) =>
    s
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '_')
      .replace(/^_+|_+$/g, '');
  return `mcp_${slug(prefix)}_${slug(tool)}`.slice(0, 64);
}

interface McpContentPart {
  type: string;
  text?: string;
}

export async function createMcpToolset(
  client: Client,
  options: McpToolsetOptions,
): Promise<AnyToolDefinition[]> {
  const { tools } = await client.listTools();
  return tools.map((tool) =>
    defineTool({
      name: mcpToolName(options.prefix, tool.name),
      title: tool.title ?? tool.annotations?.title ?? tool.name,
      description: `[Serveur MCP « ${options.prefix} »] ${tool.description ?? tool.name}`,
      inputSchema: z.record(z.string(), z.unknown()),
      inputJsonSchema: tool.inputSchema as Record<string, unknown>,
      risk: options.defaultRisk ?? 'write',
      requiredPermissions: options.defaultPermissions ?? ['connector:use'],
      describeAction: (input) =>
        `Appeler l’outil MCP « ${tool.name} » (${options.prefix}) avec ${JSON.stringify(input)}`,
      async execute(input, ctx) {
        const result = await client.callTool({ name: tool.name, arguments: input }, undefined, {
          signal: ctx.signal,
          timeout: options.timeoutMs ?? 30_000,
        });
        const parts = (Array.isArray(result.content) ? result.content : []) as McpContentPart[];
        const text = parts
          .map((p) => (p.type === 'text' ? (p.text ?? '') : `[contenu ${p.type}]`))
          .join('\n');
        return {
          content: result.structuredContent ?? text,
          isError: result.isError === true,
          summary: result.isError ? `Erreur MCP (${tool.name})` : `Réponse MCP (${tool.name})`,
        };
      },
    }),
  );
}

export type McpConnectorConfig =
  | {
      transport: 'stdio';
      prefix: string;
      command: string;
      args?: string[];
      env?: Record<string, string>;
    }
  | { transport: 'http'; prefix: string; url: string };

export const mcpConnector: ConnectorDefinition<McpConnectorConfig> = {
  type: 'mcp',
  name: 'Serveur MCP',
  description:
    'Connecte un serveur Model Context Protocol. Chaque outil exposé doit être associé à des permissions et à un niveau de risque par l’administrateur.',
  permissions: [],
  async instantiate(config, secret) {
    let transport: Transport;
    if (config.transport === 'stdio') {
      transport = new StdioClientTransport({
        command: config.command,
        ...(config.args ? { args: config.args } : {}),
        // Only explicitly configured variables reach the child process.
        ...(config.env ? { env: config.env } : {}),
      });
    } else {
      transport = new StreamableHTTPClientTransport(new URL(config.url), {
        ...(secret ? { requestInit: { headers: { Authorization: `Bearer ${secret}` } } } : {}),
      });
    }
    const client = new Client({ name: 'mimorya', version: '0.1.0' });
    await client.connect(transport);
    const tools = await createMcpToolset(client, { prefix: config.prefix });
    return { tools, close: () => client.close() };
  },
};
