/** Conversion between the provider-neutral message format and Anthropic content blocks. */
import type {
  BetaContentBlock,
  BetaContentBlockParam,
  BetaMessageParam,
} from '@anthropic-ai/sdk/resources/beta/messages/messages';
import type { LlmMessage, LlmToolCall, StopReason } from '../types';

export function toAnthropicMessages(messages: LlmMessage[]): BetaMessageParam[] {
  return messages.map((m): BetaMessageParam => {
    if (m.role === 'user') return { role: 'user', content: m.content };
    if (m.role === 'tool_results') {
      return {
        role: 'user',
        content: m.results.map((r) => ({
          type: 'tool_result' as const,
          tool_use_id: r.toolCallId,
          content: r.content,
          ...(r.isError ? { is_error: true } : {}),
        })),
      };
    }
    if (m.native?.provider === 'anthropic') {
      return { role: 'assistant', content: m.native.content as BetaContentBlockParam[] };
    }
    const blocks: BetaContentBlockParam[] = [];
    if (m.content) blocks.push({ type: 'text', text: m.content });
    for (const call of m.toolCalls) {
      blocks.push({ type: 'tool_use', id: call.id, name: call.name, input: call.input ?? {} });
    }
    return { role: 'assistant', content: blocks.length > 0 ? blocks : m.content };
  });
}

export function extractText(content: BetaContentBlock[]): string {
  return content
    .filter((b): b is Extract<BetaContentBlock, { type: 'text' }> => b.type === 'text')
    .map((b) => b.text)
    .join('');
}

export function extractToolCalls(content: BetaContentBlock[]): LlmToolCall[] {
  return content
    .filter((b): b is Extract<BetaContentBlock, { type: 'tool_use' }> => b.type === 'tool_use')
    .map((b) => ({ id: b.id, name: b.name, input: b.input }));
}

export function mapStopReason(reason: string | null): StopReason {
  switch (reason) {
    case 'end_turn':
    case 'stop_sequence':
      return 'end_turn';
    case 'tool_use':
      return 'tool_use';
    case 'max_tokens':
    case 'model_context_window_exceeded':
      return 'max_tokens';
    case 'refusal':
      return 'refusal';
    default:
      return 'other';
  }
}
