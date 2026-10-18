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
  BetaContentBlock,
  BetaContentBlockParam,
  BetaMessageParam,
  BetaToolUnion,
  MessageCreateParamsNonStreaming,
} from '@anthropic-ai/sdk/resources/beta/messages/messages';
import {
  LlmError,
  type LlmMessage,
  type LlmProvider,
  type LlmRequest,
  type LlmResponse,
  type LlmToolCall,
  type StopReason,
} from './types';

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

function toAnthropicMessages(messages: LlmMessage[]): BetaMessageParam[] {
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

function extractText(content: BetaContentBlock[]): string {
  return content
    .filter((b): b is Extract<BetaContentBlock, { type: 'text' }> => b.type === 'text')
    .map((b) => b.text)
    .join('');
}

function extractToolCalls(content: BetaContentBlock[]): LlmToolCall[] {
  return content
    .filter((b): b is Extract<BetaContentBlock, { type: 'tool_use' }> => b.type === 'tool_use')
    .map((b) => ({ id: b.id, name: b.name, input: b.input }));
}

function mapStopReason(reason: string | null): StopReason {
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

function toLlmError(error: unknown): LlmError {
  if (error instanceof LlmError) return error;
  const opts = { cause: error };
  if (error instanceof Anthropic.APIUserAbortError) {
    return new LlmError('anthropic', 'aborted', 'Requête annulée', opts);
  }
  if (
    error instanceof Anthropic.AuthenticationError ||
    error instanceof Anthropic.PermissionDeniedError
  ) {
    return new LlmError('anthropic', 'auth', 'Authentification refusée par Anthropic', opts);
  }
  if (error instanceof Anthropic.RateLimitError) {
    return new LlmError('anthropic', 'rate_limit', 'Limite de débit Anthropic atteinte', opts);
  }
  if (error instanceof Anthropic.BadRequestError) {
    const overflow = /prompt is too long|context window/i.test(error.message);
    return new LlmError(
      'anthropic',
      overflow ? 'context_overflow' : 'invalid_request',
      error.message,
      opts,
    );
  }
  if (error instanceof Anthropic.InternalServerError) {
    return new LlmError('anthropic', 'overloaded', 'Service Anthropic indisponible', opts);
  }
  if (error instanceof Anthropic.APIConnectionError) {
    return new LlmError('anthropic', 'network', 'Connexion à Anthropic impossible', opts);
  }
  if (error instanceof Anthropic.APIError) {
    return new LlmError('anthropic', 'unknown', error.message, opts);
  }
  // Not an API error: the stream could not be parsed (e.g. malformed tool input JSON).
  return new LlmError('anthropic', 'malformed_output', 'Réponse du modèle illisible', opts);
}
