import type Anthropic from '@anthropic-ai/sdk';
import { describe, expect, it } from 'vitest';
import { AnthropicProvider } from './provider';
import { PriceTable } from '../pricing';

/** Minimal stand-in for the SDK streaming helper (no network). */
function fakeClient(finalMessage: unknown) {
  const calls: unknown[] = [];
  const client = {
    beta: {
      messages: {
        stream(params: unknown) {
          calls.push(params);
          const handlers: Array<(text: string) => void> = [];
          return {
            on(event: string, handler: (text: string) => void) {
              if (event === 'text') handlers.push(handler);
              return this;
            },
            async finalMessage() {
              handlers.forEach((h) => h('Bonjour'));
              return finalMessage;
            },
          };
        },
      },
    },
  };
  return { client: client as unknown as Anthropic, calls };
}

const message = {
  model: 'claude-opus-5-5',
  stop_reason: 'tool_use',
  content: [
    { type: 'thinking', thinking: '', signature: 'sig' },
    { type: 'text', text: 'Je vérifie.' },
    { type: 'tool_use', id: 'toolu_1', name: 'jobs_search', input: { query: 'x' } },
  ],
  usage: {
    input_tokens: 1000,
    output_tokens: 200,
    cache_read_input_tokens: 5000,
    cache_creation_input_tokens: 0,
  },
};

describe('AnthropicProvider', () => {
  it('maps requests, enables caching and server-side fallback, and normalises responses', async () => {
    const { client, calls } = fakeClient(message);
    const provider = new AnthropicProvider({ client });
    const deltas: string[] = [];
    const response = await provider.complete({
      system: 'Consignes',
      effort: 'high',
      tools: [
        {
          name: 'jobs_search',
          description: 'Recherche',
          inputSchema: { type: 'object', properties: {} },
        },
      ],
      messages: [
        { role: 'user', content: 'Bonjour' },
        {
          role: 'assistant',
          content: 'Ancien',
          toolCalls: [{ id: 't0', name: 'jobs_search', input: {} }],
        },
        { role: 'tool_results', results: [{ toolCallId: 't0', content: '[]', isError: true }] },
      ],
      onTextDelta: (d) => deltas.push(d),
    });

    const params = calls[0] as Record<string, unknown>;
    expect(params).toMatchObject({
      model: 'claude-opus-5-5',
      system: 'Consignes',
      cache_control: { type: 'ephemeral' },
      output_config: { effort: 'high' },
      betas: ['server-side-fallback-2026-07-01'],
      fallbacks: 'default',
      tool_choice: { type: 'auto' },
    });
    expect(params.tools).toEqual([
      {
        name: 'jobs_search',
        description: 'Recherche',
        input_schema: { type: 'object', properties: {} },
        eager_input_streaming: true,
      },
    ]);
    expect(params.messages).toEqual([
      { role: 'user', content: 'Bonjour' },
      {
        role: 'assistant',
        content: [
          { type: 'text', text: 'Ancien' },
          { type: 'tool_use', id: 't0', name: 'jobs_search', input: {} },
        ],
      },
      {
        role: 'user',
        content: [{ type: 'tool_result', tool_use_id: 't0', content: '[]', is_error: true }],
      },
    ]);

    expect(deltas).toEqual(['Bonjour']);
    expect(response).toMatchObject({
      text: 'Je vérifie.',
      toolCalls: [{ id: 'toolu_1', name: 'jobs_search', input: { query: 'x' } }],
      stopReason: 'tool_use',
      usage: { inputTokens: 1000, outputTokens: 200, cacheReadTokens: 5000, cacheWriteTokens: 0 },
      model: 'claude-opus-5-5',
    });
    // 1000×4 + 200×20 + 5000×0.2 micro-dollars
    expect(new PriceTable().costMicros(response.model, response.usage)).toBe(9000);
  });

  it('replays assistant turns verbatim, reasoning blocks included', async () => {
    const { client, calls } = fakeClient({ ...message, stop_reason: 'end_turn', content: [] });
    const provider = new AnthropicProvider({ client });
    await provider.complete({
      system: 's',
      tools: [],
      messages: [
        { role: 'user', content: 'Q' },
        {
          role: 'assistant',
          content: 'Je vérifie.',
          toolCalls: [],
          native: { provider: 'anthropic', content: message.content },
        },
        { role: 'user', content: 'Suite' },
      ],
    });
    const params = calls[0] as { messages: Array<{ content: unknown }>; tools?: unknown };
    expect(params.messages[1]!.content).toEqual(message.content);
    expect(params.tools).toBeUndefined();
  });

  it('does not request fallbacks on models that do not support them', async () => {
    const { client, calls } = fakeClient({ ...message, stop_reason: 'end_turn' });
    await new AnthropicProvider({ client, model: 'claude-haiku-5-5' }).complete({
      system: 's',
      tools: [],
      messages: [{ role: 'user', content: 'Q' }],
    });
    expect(calls[0]).not.toHaveProperty('fallbacks');
    expect(calls[0]).not.toHaveProperty('betas');
  });
});
