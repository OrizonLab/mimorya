import { ToolExecutor, ToolRegistry, wrapUntrusted } from '@mimorya/agent';
import { InMemoryPorts } from '@mimorya/agent/testing';
import { Client } from '@modelcontextprotocol/sdk/client/index.js';
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js';
import { DEMO_USERS, demoAuthorizationEngine, demoSubject } from '@mimorya/core/demo';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { createMcpToolset, mcpToolName } from './bridge';
import { createDemoDocsServer } from './demo-docs-server';

const engine = demoAuthorizationEngine();
let client: Client;

beforeAll(async () => {
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair();
  await createDemoDocsServer().connect(serverTransport);
  client = new Client({ name: 'test', version: '0.0.0' });
  await client.connect(clientTransport);
});

afterAll(async () => {
  await client.close();
});

function executorFor(registry: ToolRegistry, userId: string) {
  return new ToolExecutor({
    engine,
    registry,
    subject: demoSubject(userId),
    actor: { type: 'agent', runId: 'r' },
    autonomy: { level: 'execute_with_approval' },
    runId: 'r',
    deliverables: new InMemoryPorts({ userId, runId: 'r' }).deliverables,
  });
}

describe('MCP bridge', () => {
  it('namespaces tool names within provider limits', () => {
    expect(mcpToolName('Docs Néo', 'search-procedures')).toBe('mcp_docs_n_o_search_procedures');
    expect(mcpToolName('x'.repeat(80), 'y').length).toBeLessThanOrEqual(64);
  });

  it('requires human validation for unmapped MCP tools (conservative default)', async () => {
    const tools = await createMcpToolset(client, { prefix: 'docs' });
    expect(tools.map((t) => t.name)).toEqual(['mcp_docs_search_procedures']);
    expect(tools[0]!.inputJsonSchema).toMatchObject({ type: 'object', required: ['query'] });

    const evaluation = executorFor(new ToolRegistry(tools), DEMO_USERS.lucas).evaluate({
      id: '1',
      name: 'mcp_docs_search_procedures',
      input: { query: 'circulateur' },
    });
    expect(evaluation.kind).toBe('require_approval');
  });

  it('runs tools mapped by the administrator as read-only, with the mapped permission', async () => {
    const tools = await createMcpToolset(client, { prefix: 'docs' });
    const registry = new ToolRegistry(tools, {
      mcp_docs_search_procedures: { risk: 'read', permissions: ['intervention:read'] },
    });

    const lucas = executorFor(registry, DEMO_USERS.lucas);
    const evaluation = lucas.evaluate({
      id: '1',
      name: 'mcp_docs_search_procedures',
      input: { query: 'circulateur' },
    });
    expect(evaluation.kind).toBe('allow');
    if (evaluation.kind !== 'allow') return;
    const { result } = await lucas.execute(evaluation.tool, evaluation.input);
    expect(result.isError).toBe(false);
    expect(wrapUntrusted('mcp', result.content)).toContain('PROC-12');

    // The administrator mapped it to intervention:read: Paul (IT admin) cannot use it.
    const paul = executorFor(registry, DEMO_USERS.paul);
    expect(
      paul.evaluate({ id: '2', name: 'mcp_docs_search_procedures', input: { query: 'x' } }).kind,
    ).toBe('deny');
  });

  it('can be disabled by the administrator', async () => {
    const tools = await createMcpToolset(client, { prefix: 'docs' });
    const registry = new ToolRegistry(tools, { mcp_docs_search_procedures: { enabled: false } });
    expect(registry.all()).toHaveLength(0);
  });
});
