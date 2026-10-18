import type { LlmProvider } from './types';

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
