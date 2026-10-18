/**
 * Cost estimation in micro-dollars (1e-6 USD). Prices are per million tokens.
 * Anthropic list prices are built in; any other model must be priced through
 * configuration (unknown models are reported as unpriced, never guessed).
 */
import type { LlmUsage } from './types';

export interface ModelPrice {
  inputPerMTok: number;
  outputPerMTok: number;
  /** Defaults to 10% of the input price. */
  cacheReadPerMTok?: number;
  /** Defaults to 125% of the input price (5-minute cache writes). */
  cacheWritePerMTok?: number;
}

export const ANTHROPIC_PRICES: Record<string, ModelPrice> = {
  'claude-fable-5-1': { inputPerMTok: 10, outputPerMTok: 50, cacheReadPerMTok: 0.25 },
  'claude-opus-5-5': { inputPerMTok: 4, outputPerMTok: 20, cacheReadPerMTok: 0.2 },
  'claude-opus-5': { inputPerMTok: 5, outputPerMTok: 25 },
  'claude-sonnet-5-5': { inputPerMTok: 2, outputPerMTok: 10, cacheReadPerMTok: 0.2 },
  'claude-sonnet-5': { inputPerMTok: 2, outputPerMTok: 10 },
  'claude-haiku-5-5': { inputPerMTok: 0.1, outputPerMTok: 0.5 },
};

export class PriceTable {
  private readonly prices: Map<string, ModelPrice>;

  constructor(extra: Record<string, ModelPrice> = {}) {
    this.prices = new Map(Object.entries({ ...ANTHROPIC_PRICES, ...extra }));
  }

  has(model: string): boolean {
    return this.prices.has(model);
  }

  /** Estimated cost in micro-dollars, or null when the model is not priced. */
  costMicros(model: string, usage: LlmUsage): number | null {
    const price = this.prices.get(model);
    if (!price) return null;
    const cacheRead = price.cacheReadPerMTok ?? price.inputPerMTok * 0.1;
    const cacheWrite = price.cacheWritePerMTok ?? price.inputPerMTok * 1.25;
    // $/MTok equals micro-dollars per token.
    const micros =
      usage.inputTokens * price.inputPerMTok +
      usage.outputTokens * price.outputPerMTok +
      usage.cacheReadTokens * cacheRead +
      usage.cacheWriteTokens * cacheWrite;
    return Math.round(micros);
  }
}
