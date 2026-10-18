import { AnthropicProvider, DEFAULT_ANTHROPIC_MODEL } from './anthropic';
import { OpenAICompatibleProvider } from './openai-compatible';
import type { LlmProvider } from './types';

export const PROVIDER_KINDS = [
  'anthropic',
  'openai',
  'openrouter',
  'gemini',
  'ollama',
  'openai_compatible',
] as const;
export type ProviderKind = (typeof PROVIDER_KINDS)[number];

export interface ProviderConfig {
  kind: ProviderKind;
  model?: string;
  apiKey?: string;
  baseURL?: string;
}

const OPENAI_COMPATIBLE_ENDPOINTS: Partial<Record<ProviderKind, string>> = {
  openrouter: 'https://openrouter.ai/api/v1',
  gemini: 'https://generativelanguage.googleapis.com/v1beta/openai/',
  ollama: 'http://localhost:11434/v1',
};

export function createProvider(config: ProviderConfig): LlmProvider {
  if (config.kind === 'anthropic') {
    return new AnthropicProvider({
      model: config.model ?? DEFAULT_ANTHROPIC_MODEL,
      ...(config.apiKey ? { apiKey: config.apiKey } : {}),
      ...(config.baseURL ? { baseURL: config.baseURL } : {}),
    });
  }
  if (!config.model) {
    throw new Error(`Provider "${config.kind}" requires an explicit model name`);
  }
  const baseURL = config.baseURL ?? OPENAI_COMPATIBLE_ENDPOINTS[config.kind];
  if (config.kind === 'openai_compatible' && !baseURL) {
    throw new Error('Provider "openai_compatible" requires a base URL');
  }
  return new OpenAICompatibleProvider({
    id: config.kind,
    model: config.model,
    ...(config.apiKey ? { apiKey: config.apiKey } : {}),
    ...(baseURL ? { baseURL } : {}),
  });
}

export type TaskClass = 'agent' | 'verification';

/**
 * Chooses a provider per task class. Tenants can route verification to a
 * different (cheaper, or local) model; by default every class uses the
 * agent provider.
 */
export class ModelRouter {
  constructor(
    private readonly providers: { agent: LlmProvider } & Partial<Record<TaskClass, LlmProvider>>,
  ) {}

  for(task: TaskClass): LlmProvider {
    return this.providers[task] ?? this.providers.agent;
  }
}
