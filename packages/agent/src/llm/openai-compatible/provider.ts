/**
 * Adapter for OpenAI-compatible Chat Completions endpoints: OpenAI,
 * OpenRouter, Google Gemini (OpenAI-compatible endpoint), Mistral, and local
 * runtimes (Ollama, vLLM, LM Studio). Non-streaming for maximal compatibility
 * across servers; the final text is reported as a single delta.
 */
import OpenAI from 'openai';
import type { ChatCompletionTool } from 'openai/resources/chat/completions/completions';
import {
  LlmError,
  type LlmProvider,
  type LlmRequest,
  type LlmResponse,
  type LlmToolCall,
} from '../types';
import { toLlmError } from './errors';
import { mapFinishReason, parseArguments, toOpenAIMessages } from './messages';

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
