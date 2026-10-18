/**
 * Provider-neutral LLM contract. Adapters translate to and from each
 * provider's wire format; the runtime only sees these types.
 */

export interface LlmToolCall {
  id: string;
  name: string;
  /** Parsed arguments, still unvalidated: the policy enforcement point validates them. */
  input: unknown;
}

export interface LlmToolResult {
  toolCallId: string;
  content: string;
  isError?: boolean;
}

/**
 * Assistant turns keep the provider's native content so they can be replayed
 * verbatim on the next request (required by providers that bind reasoning
 * blocks to the conversation, and good for prompt caching).
 */
export interface ProviderNativeContent {
  provider: string;
  content: unknown;
}

export type LlmMessage =
  | { role: 'user'; content: string }
  | { role: 'assistant'; content: string; toolCalls: LlmToolCall[]; native?: ProviderNativeContent }
  | { role: 'tool_results'; results: LlmToolResult[] };

export interface LlmToolSpec {
  name: string;
  description: string;
  inputSchema: Record<string, unknown>;
}

export type Effort = 'low' | 'medium' | 'high' | 'xhigh' | 'max';

export interface LlmRequest {
  /** Stable instructions. Keep volatile data out of it to preserve prompt caching. */
  system: string;
  messages: LlmMessage[];
  tools: LlmToolSpec[];
  maxOutputTokens?: number;
  effort?: Effort;
  signal?: AbortSignal;
  onTextDelta?: (delta: string) => void;
}

export type StopReason = 'end_turn' | 'tool_use' | 'max_tokens' | 'refusal' | 'other';

export interface LlmUsage {
  inputTokens: number;
  outputTokens: number;
  cacheReadTokens: number;
  cacheWriteTokens: number;
}

export interface LlmResponse {
  text: string;
  toolCalls: LlmToolCall[];
  stopReason: StopReason;
  usage: LlmUsage;
  provider: string;
  /** Model that actually served the request (may differ after a server-side fallback). */
  model: string;
  native?: ProviderNativeContent;
}

export interface LlmProvider {
  readonly id: string;
  readonly model: string;
  complete(request: LlmRequest): Promise<LlmResponse>;
}

export type LlmErrorKind =
  | 'auth'
  | 'rate_limit'
  | 'overloaded'
  | 'invalid_request'
  | 'context_overflow'
  | 'network'
  | 'malformed_output'
  | 'aborted'
  | 'unknown';

export class LlmError extends Error {
  readonly kind: LlmErrorKind;
  readonly retryable: boolean;
  readonly provider: string;

  constructor(
    provider: string,
    kind: LlmErrorKind,
    message: string,
    options?: { cause?: unknown },
  ) {
    super(message, options);
    this.name = 'LlmError';
    this.provider = provider;
    this.kind = kind;
    this.retryable =
      kind === 'rate_limit' ||
      kind === 'overloaded' ||
      kind === 'network' ||
      kind === 'malformed_output';
  }
}

export const EMPTY_USAGE: LlmUsage = {
  inputTokens: 0,
  outputTokens: 0,
  cacheReadTokens: 0,
  cacheWriteTokens: 0,
};
