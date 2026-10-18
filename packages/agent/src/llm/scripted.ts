/**
 * Deterministic provider for tests: each call consumes the next scripted
 * step. A step may inspect the request (visible tools, last tool results) to
 * decide its answer, which lets tests assert on what the model was shown.
 */
import {
  EMPTY_USAGE,
  type LlmProvider,
  type LlmRequest,
  type LlmResponse,
  type LlmToolCall,
  type LlmToolResult,
} from './types';

export interface ScriptedStepResult {
  text: string;
  toolCalls?: Array<Omit<LlmToolCall, 'id'> & { id?: string }>;
}

export type ScriptedStep = ScriptedStepResult | ((request: LlmRequest) => ScriptedStepResult);

export class ScriptedProvider implements LlmProvider {
  readonly id = 'scripted';
  readonly model = 'scripted-model';
  readonly requests: LlmRequest[] = [];
  private index = 0;

  constructor(private readonly steps: ScriptedStep[]) {}

  get remaining(): number {
    return this.steps.length - this.index;
  }

  async complete(request: LlmRequest): Promise<LlmResponse> {
    this.requests.push(structuredClone({ ...request, signal: undefined, onTextDelta: undefined }));
    const step = this.steps[this.index++];
    if (!step) throw new Error(`ScriptedProvider: no step left (call #${this.index})`);
    const result = typeof step === 'function' ? step(request) : step;
    const toolCalls = (result.toolCalls ?? []).map((c, i) => ({
      id: c.id ?? `call_${this.index}_${i}`,
      name: c.name,
      input: c.input,
    }));
    if (result.text) request.onTextDelta?.(result.text);
    return {
      text: result.text,
      toolCalls,
      stopReason: toolCalls.length > 0 ? 'tool_use' : 'end_turn',
      usage: { ...EMPTY_USAGE, inputTokens: 100, outputTokens: 20 },
      provider: this.id,
      model: this.model,
    };
  }
}

/** Last tool results visible in a request (helper for scripted steps). */
export function lastToolResults(request: LlmRequest): LlmToolResult[] {
  const last = request.messages[request.messages.length - 1];
  return last?.role === 'tool_results' ? last.results : [];
}
