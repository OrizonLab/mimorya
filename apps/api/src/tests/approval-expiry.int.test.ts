/** An expired approval is recorded as such and reported to the agent as a refusal. */
import { DEMO_TENANT, DEMO_USERS } from '@mimorya/core/demo';
import { withTenant } from '@mimorya/db';
import { sql } from 'drizzle-orm';
import { describe, expect, it } from 'vitest';
import { json, NEXT_MONDAY, useTestApi } from './harness';

const api = useTestApi();
const { as, login } = api;

describe('approval expiry', () => {
  it('reports an expired validation to the agent as a refusal and resumes the run', async () => {
    const marc = as(await login(DEMO_USERS.marc));
    api.steps.push(
      {
        text: '',
        toolCalls: [
          {
            name: 'planning_propose',
            input: {
              title: 'Suivi INT-2052',
              items: [
                {
                  interventionId: 'INT-2052',
                  technicianId: DEMO_USERS.lea,
                  date: NEXT_MONDAY,
                  startTime: '15:00',
                  durationHours: 1,
                  reason: 'Entretien à reprogrammer',
                },
              ],
            },
          },
        ],
      },
      (req) => {
        const last = req.messages[req.messages.length - 1]!;
        const content = last.role === 'tool_results' ? last.results[0]!.content : '';
        const proposalId = /"propositionId": "([0-9a-f-]+)"/.exec(content)![1]!;
        return { text: '', toolCalls: [{ name: 'planning_apply', input: { proposalId } }] };
      },
      (req) => {
        const last = req.messages[req.messages.length - 1]!;
        expect(last.role === 'tool_results' && last.results[0]!.content).toContain('expirée');
        return { text: 'La validation a expiré : le planning de INT-2052 n’a pas été appliqué.' };
      },
    );
    const conv = json<{ conversation: { id: string } }>(await marc.post('/conversations'));
    const { runId } = json<{ runId: string }>(
      await marc.post(`/conversations/${conv.conversation.id}/messages`, {
        content: 'Planifie INT-2052',
      }),
    );
    await api.app.services.runs.idle();
    const [approval] = json<{ approvals: Array<{ id: string; runId: string }> }>(
      await marc.get('/approvals'),
    ).approvals.filter((a) => a.runId === runId);

    await withTenant(api.app.services.db, DEMO_TENANT.id, (tx) =>
      tx.execute(
        sql`update approvals set expires_at = now() - interval '1 minute' where id = ${approval!.id}`,
      ),
    );
    const decision = await marc.post(`/approvals/${approval!.id}/decision`, { approved: true });
    expect(decision.statusCode).toBe(409);
    await api.app.services.runs.idle();

    const detail = json<{ run: { status: string }; approvals: Array<{ status: string }> }>(
      await marc.get(`/runs/${runId}`),
    );
    expect(detail.run.status).toBe('completed');
    expect(detail.approvals.map((a) => a.status)).toEqual(['expired']);
  });
});
