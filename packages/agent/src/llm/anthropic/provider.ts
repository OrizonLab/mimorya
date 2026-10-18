/**
 * Anthropic adapter (Messages API, official SDK).
 *
 * - Streams every request (long agentic turns) and reports text deltas.
 * - Replays assistant turns verbatim from their native content blocks, as
 *   required for reasoning blocks bound to the conversation.
 * - Enables prompt caching on the stable prefix (tools + system).
 * - Opts into server-side fallback on supported models so that a policy
 *   decline is retried on the server-defined fallback model.
 */
import Anthropic from '@anthropic-ai/sdk';
import type {
  BetaToolUnion,
  MessageCreateParamsNonStreaming,
} from '@anthropic-ai/sdk/resources/beta/messages/messages';
import type { LlmProvider, LlmRequest, LlmResponse } from '../types';
import { toLlmError } from './errors';
import { extractText, extractToolCalls, mapStopReason, toAnthropicMessages } from './messages';

export const DEFAULT_ANTHROPIC_MODEL = 'claude-opus-5-5';

/** Models accepting `fallbacks: "default"` on the Claude API. */
const SERVER_FALLBACK_MODELS = new Set([
  'claude-fable-5-1',
  'claude-opus-5-5',
  'claude-opus-5',
  'claude-sonnet-5-5',
]);
const SERVER_FALLBACK_BETA = 'server-side-fallback-2026-07-01';

export interface AnthropicProviderOptions {
  apiKey?: string;
  model?: string;
  baseURL?: string;
  /** Default 64k: requests are streamed, so large caps do not hit HTTP timeouts. */
  maxOutputTokens?: number;
  serverSideFallback?: boolean;
  /** Stream tool inputs as generated; inputs are validated by the runtime anyway. */
  eagerInputStreaming?: boolean;
  client?: Anthropic;
}

export class AnthropicProvider implements LlmProvider {
  readonly id = 'anthropic';
  readonly model: string;
  private readonly client: Anthropic;
  private readonly maxOutputTokens: number;
  private readonly serverSideFallback: boolean;
  private readonly eagerInputStreaming: boolean;

  constructor(options: AnthropicProviderOptions = {}) {
    this.model = options.model ?? DEFAULT_ANTHROPIC_MODEL;
    this.client =
      options.client ??
      new Anthropic({
        ...(options.apiKey ? { apiKey: options.apiKey } : {}),
        ...(options.baseURL ? { baseURL: options.baseURL } : {}),
      });
    this.maxOutputTokens = options.maxOutputTokens ?? 64_000;
    this.serverSideFallback =
      (options.serverSideFallback ?? true) && SERVER_FALLBACK_MODELS.has(this.model);
    this.eagerInputStreaming = options.eagerInputStreaming ?? true;
  }

  async complete(request: LlmRequest): Promise<LlmResponse> {
    const tools: BetaToolUnion[] = request.tools.map((t) => ({
      name: t.name,
      description: t.description,
      input_schema: t.inputSchema as Extract<
        BetaToolUnion,
        { input_schema: unknown }
      >['input_schema'],
      ...(this.eagerInputStreaming ? { eager_input_streaming: true } : {}),
    }));

    const params: MessageCreateParamsNonStreaming = {
      model: this.model,
      max_tokens: request.maxOutputTokens ?? this.maxOutputTokens,
      system: request.system,
      messages: toAnthropicMessages(request.messages),
      cache_control: { type: 'ephemeral' },
      ...(tools.length > 0 ? { tools, tool_choice: { type: 'auto' } } : {}),
      ...(request.effort ? { output_config: { effort: request.effort } } : {}),
      ...(this.serverSideFallback ? { betas: [SERVER_FALLBACK_BETA], fallbacks: 'default' } : {}),
    };

    try {
      const stream = this.client.beta.messages.stream(params, { signal: request.signal });
      if (request.onTextDelta) stream.on('text', (delta) => request.onTextDelta?.(delta));
      const message = await stream.finalMessage();
      return {
        text: extractText(message.content),
        toolCalls: extractToolCalls(message.content),
        stopReason: mapStopReason(message.stop_reason),
        usage: {
          inputTokens: message.usage.input_tokens,
          outputTokens: message.usage.output_tokens,
          cacheReadTokens: message.usage.cache_read_input_tokens ?? 0,
          cacheWriteTokens: message.usage.cache_creation_input_tokens ?? 0,
        },
        provider: this.id,
        model: message.model,
        native: { provider: this.id, content: message.content },
      };
    } catch (error) {
      throw toLlmError(error);
    }
  }
}
