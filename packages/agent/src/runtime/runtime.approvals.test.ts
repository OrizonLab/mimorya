/** Runtime: human validation of side-effecting tool calls. */
import { describe, expect, it } from 'vitest';
import { lastToolResults } from '../testing';
import { applied, setup } from './runtime.fixtures';

describe('AgentRuntime — human validation', () => {
  it('pauses for human validation and executes exactly the validated parameters', async () => {
    applied.length = 0;
    const { runtime, start, ports } = setup([
      {
        text: 'Je prépare l’application.',
        toolCalls: [
          { name: 'jobs_search', input: { query: 'x' }, id: 'c1' },
          { name: 'planning_apply', input: { slots: 3 }, id: 'c2' },
        ],
      },
      (req) => {
        const results = lastToolResults(req);
        expect(results.map((r) => r.toolCallId)).toEqual(['c1', 'c2']);
        expect(results[1]!.content).toContain('ok');
        return { text: 'Planning appliqué pour INT-0001.' };
      },
    ]);
    const first = await start();
    expect(first.status).toBe('awaiting_approval');
    if (first.status !== 'awaiting_approval') return;
    expect(applied).toHaveLength(0);
    expect(ports.approvalRequests[0]).toMatchObject({
      toolName: 'planning_apply',
      actionSummary: 'Appliquer 3 créneau(x)',
      risk: 'write',
    });

    const approvalId = first.approvals[0]!.approvalId;
    // Still waiting while no decision has been taken.
    expect((await runtime.resume(first.checkpoint, {})).status).toBe('awaiting_approval');

    const second = await runtime.resume(first.checkpoint, { [approvalId]: { approved: true } });
    expect(second.status).toBe('completed');
    expect(applied).toEqual([{ slots: 3 }]);
    expect(
      ports.toolRecords.find((r) => r.toolName === 'planning_apply' && r.status === 'succeeded'),
    ).toMatchObject({
      approvalId,
    });
  });

  it('reports a rejected validation to the model', async () => {
    applied.length = 0;
    const { runtime, start } = setup([
      { text: '', toolCalls: [{ name: 'planning_apply', input: { slots: 1 } }] },
      (req) => {
        expect(lastToolResults(req)[0]!.content).toMatch(
          /refusée par l’utilisateur : pas cette semaine/,
        );
        return { text: 'Entendu, je n’applique pas le planning.' };
      },
    ]);
    const first = await start();
    if (first.status !== 'awaiting_approval') throw new Error('expected approval');
    const outcome = await runtime.resume(first.checkpoint, {
      [first.approvals[0]!.approvalId]: { approved: false, comment: 'pas cette semaine' },
    });
    expect(outcome.status).toBe('completed');
    expect(applied).toHaveLength(0);
  });
});
