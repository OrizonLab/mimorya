/** Conversion between the provider-neutral message format and Chat Completions messages. */
import type { ChatCompletionMessageParam } from 'openai/resources/chat/completions/completions';
import type { LlmMessage, StopReason } from '../types';

export function parseArguments(raw: string): unknown {
  try {
    return raw ? JSON.parse(raw) : {};
  } catch {
    // Returned as-is: schema validation will reject it and tell the model.
    return { __invalid_json__: raw };
  }
}

export function toOpenAIMessages(messages: LlmMessage[]): ChatCompletionMessageParam[] {
  const result: ChatCompletionMessageParam[] = [];
  for (const m of messages) {
    if (m.role === 'user') {
      result.push({ role: 'user', content: m.content });
    } else if (m.role === 'assistant') {
      result.push({
        role: 'assistant',
        content: m.content || null,
        ...(m.toolCalls.length > 0
          ? {
              tool_calls: m.toolCalls.map((c) => ({
                id: c.id,
                type: 'function' as const,
                function: { name: c.name, arguments: JSON.stringify(c.input ?? {}) },
              })),
            }
          : {}),
      });
    } else {
      for (const r of m.results) {
        result.push({ role: 'tool', tool_call_id: r.toolCallId, content: r.content });
      }
    }
  }
  return result;
}

export function mapFinishReason(
  reason: string | null | undefined,
  hasToolCalls: boolean,
): StopReason {
  if (hasToolCalls || reason === 'tool_calls') return 'tool_use';
  if (reason === 'stop') return 'end_turn';
  if (reason === 'length') return 'max_tokens';
  if (reason === 'content_filter') return 'refusal';
  return 'other';
}
