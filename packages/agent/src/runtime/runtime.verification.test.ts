/** Runtime: verification of final answers, bounded correction and budgets. */
import { describe, expect, it } from 'vitest';
import type { ScriptedStep } from '../testing';
import { setup } from './runtime.fixtures';

describe('AgentRuntime — verification and budgets', () => {
  it('detects ungrounded identifiers and corrects the answer', async () => {
    const { start, ports, provider } = setup([
      { text: '', toolCalls: [{ name: 'jobs_search', input: { query: 'x' } }] },
      { text: 'Interventions : INT-0001 et INT-0999.' },
      (req) => {
        const last = req.messages[req.messages.length - 1]!;
        expect(last.role === 'user' && last.content).toMatch(/INT-0999 est cité/);
        return { text: 'Intervention : INT-0001.' };
      },
    ]);
    const outcome = await start();
    expect(outcome.status).toBe('completed');
    if (outcome.status !== 'completed') return;
    expect(outcome.verification.passed).toBe(true);
    expect(provider.remaining).toBe(0);
    expect(ports.eventsOfType('verification').map((v) => v.passed)).toEqual([false, true]);
  });

  it('never hides residual verification issues', async () => {
    const { start } = setup([
      { text: 'INT-0999' },
      { text: 'INT-0999 toujours' },
      { text: 'INT-0999 encore' },
    ]);
    const outcome = await start();
    if (outcome.status !== 'completed') throw new Error('expected completion');
    expect(outcome.verification.passed).toBe(false);
    expect(outcome.answer).toMatch(/Points non vérifiés/);
  });

  it('uses the model review against explicit success criteria', async () => {
    const { start, judge } = setup(
      [
        { text: '', toolCalls: [{ name: 'jobs_search', input: { query: 'x' } }] },
        { text: 'INT-0001.' },
        { text: 'INT-0001 : chaudière remise en service, aucun suivi nécessaire.' },
      ],
      {
        criteria: ['Indiquer pour chaque intervention si un suivi est nécessaire'],
        judge: [
          {
            text: '',
            toolCalls: [
              { name: 'submit_verdict', input: { pass: false, issues: ['Suivi non précisé'] } },
            ],
          },
          { text: '', toolCalls: [{ name: 'submit_verdict', input: { pass: true, issues: [] } }] },
        ],
      },
    );
    const outcome = await start();
    if (outcome.status !== 'completed') throw new Error('expected completion');
    expect(outcome.verification).toEqual({ passed: true, issues: [], judged: true });
    expect(judge!.requests[0]!.messages[0]).toMatchObject({ role: 'user' });
  });

  it('stops when the step budget is exhausted', async () => {
    const loop: ScriptedStep = {
      text: '',
      toolCalls: [{ name: 'jobs_search', input: { query: 'x' } }],
    };
    const { start, ports } = setup([loop, loop, loop, loop], { budget: { maxSteps: 2 } });
    const outcome = await start();
    expect(outcome.status).toBe('failed');
    if (outcome.status !== 'failed') return;
    expect(outcome.error).toMatch(/nombre maximal d’étapes/);
    expect(ports.eventsOfType('run_finished')).toEqual([
      { type: 'run_finished', status: 'failed', error: outcome.error },
    ]);
  });
});
