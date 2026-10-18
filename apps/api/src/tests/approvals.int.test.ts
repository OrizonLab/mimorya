/**
 * Decisions on approval requests: an expired request counts as a refusal, and
 * a decision is only recorded while the run is actually paused for it.
 */
import type { ScriptedStep } from '@mimorya/agent/testing';
import { DEMO_TENANT, DEMO_USERS } from '@mimorya/core/demo';
import { withTenant } from '@mimorya/db';
import { sql, type SQL } from 'drizzle-orm';
import { describe, expect, it } from 'vitest';
import { json, NEXT_MONDAY, useTestApi, type SessionClient } from './harness';

const api = useTestApi();
const { as, login } = api;

const execute = (query: SQL) =>
  withTenant(api.app.services.db, DEMO_TENANT.id, (tx) => tx.execute(query));

/** Scripts a proposal then its application, and returns the paused run. */
async function pausedRun(
  marc: SessionClient,
  slot: { interventionId: string; startTime: string },
  finalStep: ScriptedStep,
) {
  api.steps.push(
    {
      text: '',
      toolCalls: [
        {
          name: 'planning_propose',
          input: {
            title: `Suivi ${slot.interventionId}`,
            items: [
              {
                ...slot,
                technicianId: DEMO_USERS.lea,
                date: NEXT_MONDAY,
                durationHours: 1,
                reason: 'Passage à reprogrammer',
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
    finalStep,
  );
  const conv = json<{ conversation: { id: string } }>(await marc.post('/conversations'));
  const { runId } = json<{ runId: string }>(
    await marc.post(`/conversations/${conv.conversation.id}/messages`, {
      content: `Planifie ${slot.interventionId}`,
    }),
  );
  await api.app.services.runs.idle();
  const [approval] = json<{ approvals: Array<{ id: string; runId: string }> }>(
    await marc.get('/approvals'),
  ).approvals.filter((a) => a.runId === runId);
  return { runId, approvalId: approval!.id };
}

async function runDetail(marc: SessionClient, runId: string) {
  return json<{ run: { status: string }; approvals: Array<{ status: string }> }>(
    await marc.get(`/runs/${runId}`),
  );
}

describe('approval decisions', () => {
  it('reports an expired validation to the agent as a refusal and resumes the run', async () => {
    const marc = as(await login(DEMO_USERS.marc));
    const { runId, approvalId } = await pausedRun(
      marc,
      { interventionId: 'INT-2052', startTime: '15:00' },
      (req) => {
        const last = req.messages[req.messages.length - 1]!;
        expect(last.role === 'tool_results' && last.results[0]!.content).toContain('expirée');
        return { text: 'La validation a expiré : le planning de INT-2052 n’a pas été appliqué.' };
      },
    );

    await execute(
      sql`update approvals set expires_at = now() - interval '1 minute' where id = ${approvalId}`,
    );
    const decision = await marc.post(`/approvals/${approvalId}/decision`, { approved: true });
    expect(decision.statusCode).toBe(409);
    await api.app.services.runs.idle();

    const detail = await runDetail(marc, runId);
    expect(detail.run.status).toBe('completed');
    expect(detail.approvals.map((a) => a.status)).toEqual(['expired']);
  });

  it('records a decision only while the run is paused for it', async () => {
    const marc = as(await login(DEMO_USERS.marc));
    const { runId, approvalId } = await pausedRun(
      marc,
      { interventionId: 'INT-2049', startTime: '09:00' },
      { text: 'Le planning de INT-2049 est appliqué.' },
    );
    const decide = () => marc.post(`/approvals/${approvalId}/decision`, { approved: true });

    // The run has not saved its pause yet: nothing would resume it.
    await execute(sql`update agent_runs set status = 'running' where id = ${runId}`);
    const early = await decide();
    expect(early.statusCode).toBe(409);
    expect(json<{ error: { message: string } }>(early).error.message).toMatch(/préparation/);

    // The run has ended in the meantime: the action can no longer be carried out.
    await execute(sql`update agent_runs set status = 'failed' where id = ${runId}`);
    const late = await decide();
    expect(late.statusCode).toBe(409);
    expect(json<{ error: { message: string } }>(late).error.message).toMatch(/terminée/);
    expect((await runDetail(marc, runId)).approvals.map((a) => a.status)).toEqual(['pending']);

    // Once paused, the same decision is recorded and resumes the run.
    await execute(sql`update agent_runs set status = 'awaiting_approval' where id = ${runId}`);
    const accepted = await decide();
    expect(accepted.statusCode).toBe(200);
    expect(json<{ resumed: boolean }>(accepted).resumed).toBe(true);
    await api.app.services.runs.idle();

    const detail = await runDetail(marc, runId);
    expect(detail.run.status).toBe('completed');
    expect(detail.approvals.map((a) => a.status)).toEqual(['approved']);
    expect((await decide()).statusCode).toBe(409);
  });
});
