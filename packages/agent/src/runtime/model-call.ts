/** Model calls with bounded retries, and the accounting of what they cost. */
import type { PriceTable } from '../llm/pricing';
import { LlmError, type LlmProvider, type LlmRequest, type LlmResponse } from '../llm/types';
import type { EnginePorts } from './ports';
import type { RunCheckpoint } from './state';

export interface RetryPolicy {
  maxRetries: number;
  delayMs: number;
}

/** Retries retryable provider errors with exponential backoff; other errors propagate. */
export async function completeWithRetry(
  provider: LlmProvider,
  request: LlmRequest,
  policy: RetryPolicy,
): Promise<LlmResponse> {
  for (let attempt = 0; ; attempt++) {
    try {
      return await provider.complete(request);
    } catch (error) {
      if (!(error instanceof LlmError) || !error.retryable || attempt >= policy.maxRetries) {
        throw error;
      }
      await new Promise((resolve) => setTimeout(resolve, policy.delayMs * 2 ** attempt));
    }
  }
}

/** Adds a model response to the run totals, records it for metering and notifies listeners. */
export async function recordModelUsage(
  ports: EnginePorts,
  prices: PriceTable,
  state: RunCheckpoint,
  response: LlmResponse,
): Promise<void> {
  const cost = prices.costMicros(response.model, response.usage);
  const inputTokens =
    response.usage.inputTokens + response.usage.cacheReadTokens + response.usage.cacheWriteTokens;
  state.usage.inputTokens += inputTokens;
  state.usage.outputTokens += response.usage.outputTokens;
  state.usage.costMicros += cost ?? 0;
  await ports.usage.record({
    kind: 'llm',
    provider: response.provider,
    model: response.model,
    inputTokens,
    outputTokens: response.usage.outputTokens,
    costMicros: cost ?? 0,
  });
  await ports.events.emit({
    type: 'usage',
    inputTokens: response.usage.inputTokens,
    outputTokens: response.usage.outputTokens,
    costMicros: cost,
  });
}
