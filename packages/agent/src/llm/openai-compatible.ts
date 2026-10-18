/**
 * Adapter for OpenAI-compatible Chat Completions endpoints: OpenAI,
 * OpenRouter, Google Gemini (OpenAI-compatible endpoint), Mistral, and local
 * runtimes (Ollama, vLLM, LM Studio). Non-streaming for maximal compatibility
 * across servers; the final text is reported as a single delta.
 */
import OpenAI from 'openai';
import type {
  ChatCompletionMessageParam,
  ChatCompletionTool,
} from 'openai/resources/chat/completions/completions';
import {
  LlmError,
  type LlmMessage,
  type LlmProvider,
  type LlmRequest,
  type LlmResponse,
  type LlmToolCall,
  type StopReason,
} from './types';

export interface OpenAICompatibleOptions {
  /** Identifier used in usage records, e.g. "openai", "openrouter", "ollama". */
  id?: string;
  model: string;
  apiKey?: string;
  baseURL?: string;
  client?: OpenAI;
}

export class OpenAICompatibleProvider implements LlmProvider {
  readonly id: string;
  readonly model: string;
  private readonly client: OpenAI;

  constructor(options: OpenAICompatibleOptions) {
    this.id = options.id ?? 'openai';
    this.model = options.model;
    this.client =
      options.client ??
      new OpenAI({
        // Local runtimes accept any key; the SDK requires a non-empty value.
        apiKey: options.apiKey ?? 'not-needed',
        ...(options.baseURL ? { baseURL: options.baseURL } : {}),
      });
  }

  async complete(request: LlmRequest): Promise<LlmResponse> {
    const tools: ChatCompletionTool[] = request.tools.map((t) => ({
      type: 'function',
      function: { name: t.name, description: t.description, parameters: t.inputSchema },
    }));

    try {
      const completion = await this.client.chat.completions.create(
        {
          model: this.model,
          messages: [
            { role: 'system', content: request.system },
            ...toOpenAIMessages(request.messages),
          ],
          ...(tools.length > 0 ? { tools, tool_choice: 'auto' as const } : {}),
        },
        { signal: request.signal },
      );
      const choice = completion.choices[0];
      if (!choice) throw new LlmError(this.id, 'malformed_output', 'Réponse vide du modèle');
      const text = choice.message.content ?? '';
      if (text && request.onTextDelta) request.onTextDelta(text);

      const toolCalls: LlmToolCall[] = [];
      for (const call of choice.message.tool_calls ?? []) {
        if (call.type !== 'function') continue;
        toolCalls.push({
          id: call.id,
          name: call.function.name,
          input: parseArguments(call.function.arguments),
        });
      }

      return {
        text,
        toolCalls,
        stopReason: mapFinishReason(choice.finish_reason, toolCalls.length > 0),
        usage: {
          inputTokens: completion.usage?.prompt_tokens ?? 0,
          outputTokens: completion.usage?.completion_tokens ?? 0,
          cacheReadTokens: completion.usage?.prompt_tokens_details?.cached_tokens ?? 0,
          cacheWriteTokens: 0,
        },
        provider: this.id,
        model: completion.model || this.model,
      };
    } catch (error) {
      throw toLlmError(this.id, error);
    }
  }
}

function parseArguments(raw: string): unknown {
  try {
    return raw ? JSON.parse(raw) : {};
  } catch {
    // Returned as-is: schema validation will reject it and tell the model.
    return { __invalid_json__: raw };
  }
}

function toOpenAIMessages(messages: LlmMessage[]): ChatCompletionMessageParam[] {
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

function mapFinishReason(reason: string | null | undefined, hasToolCalls: boolean): StopReason {
  if (hasToolCalls || reason === 'tool_calls') return 'tool_use';
  if (reason === 'stop') return 'end_turn';
  if (reason === 'length') return 'max_tokens';
  if (reason === 'content_filter') return 'refusal';
  return 'other';
}

function toLlmError(provider: string, error: unknown): LlmError {
  if (error instanceof LlmError) return error;
  const opts = { cause: error };
  if (error instanceof OpenAI.APIUserAbortError) {
    return new LlmError(provider, 'aborted', 'Requête annulée', opts);
  }
  if (
    error instanceof OpenAI.AuthenticationError ||
    error instanceof OpenAI.PermissionDeniedError
  ) {
    return new LlmError(provider, 'auth', 'Authentification refusée par le fournisseur', opts);
  }
  if (error instanceof OpenAI.RateLimitError) {
    return new LlmError(provider, 'rate_limit', 'Limite de débit atteinte', opts);
  }
  if (error instanceof OpenAI.BadRequestError) {
    const overflow = /context length|too long|maximum context/i.test(error.message);
    return new LlmError(
      provider,
      overflow ? 'context_overflow' : 'invalid_request',
      error.message,
      opts,
    );
  }
  if (error instanceof OpenAI.InternalServerError) {
    return new LlmError(provider, 'overloaded', 'Fournisseur indisponible', opts);
  }
  if (error instanceof OpenAI.APIConnectionError) {
    return new LlmError(provider, 'network', 'Connexion au fournisseur impossible', opts);
  }
  if (error instanceof OpenAI.APIError)
    return new LlmError(provider, 'unknown', error.message, opts);
  return new LlmError(
    provider,
    'unknown',
    error instanceof Error ? error.message : String(error),
    opts,
  );
}
