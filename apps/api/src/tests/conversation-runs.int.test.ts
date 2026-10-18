/**
 * The conversation → run → approval → resume cycle with a scripted model,
 * including authorization of other users and SSE replay.
 */
import { DEMO_USERS } from '@mimorya/core/demo';
import { describe, expect, it } from 'vitest';
import { json, NEXT_MONDAY, useTestApi, WEEK } from './harness';

const api = useTestApi();
const { as, login } = api;

describe('conversations and runs', () => {
  it('refuses to start a run when no model provider is configured', async () => {
    const marc = as(await login(DEMO_USERS.marc, api.appWithoutLlm), api.appWithoutLlm);
    const conv = json<{ conversation: { id: string } }>(await marc.post('/conversations'));
    const res = await marc.post(`/conversations/${conv.conversation.id}/messages`, {
      content: 'Bonjour',
    });
    expect(res.statusCode).toBe(503);
  });

  it('runs a conversation turn, pauses for validation and resumes after the decision', async () => {
    const marcSession = await login(DEMO_USERS.marc);
    const marc = as(marcSession);
    const julien = as(await login(DEMO_USERS.julien));
    let proposalId = '';

    api.steps.push(
      {
        text: '',
        toolCalls: [{ name: 'interventions_search', input: { ...WEEK, status: ['in_progress'] } }],
      },
      {
        text: '',
        toolCalls: [
          {
            name: 'planning_propose',
            input: {
              title: 'Suivi',
              items: [
                {
                  interventionId: 'INT-2049',
                  technicianId: DEMO_USERS.lea,
                  date: NEXT_MONDAY,
                  startTime: '13:00',
                  durationHours: 2,
                  reason: 'Passage avec le gardien',
                },
              ],
            },
          },
        ],
      },
      (req) => {
        const last = req.messages[req.messages.length - 1]!;
        const content = last.role === 'tool_results' ? last.results[0]!.content : '';
        proposalId = /"propositionId": "([0-9a-f-]+)"/.exec(content)![1]!;
        return {
          text: 'Je demande la validation.',
          toolCalls: [{ name: 'planning_apply', input: { proposalId } }],
        };
      },
      { text: 'Planning appliqué : suivi de INT-2049 programmé.' },
    );

    const conv = json<{ conversation: { id: string } }>(await marc.post('/conversations'));
    const started = await marc.post(`/conversations/${conv.conversation.id}/messages`, {
      content: 'Propose un suivi pour les interventions en cours',
    });
    expect(started.statusCode).toBe(202);
    const { runId } = json<{ runId: string }>(started);
    await api.app.services.runs.idle();

    const pending = json<{
      approvals: Array<{ id: string; toolName: string; actionSummary: string }>;
    }>(await marc.get('/approvals'));
    expect(pending.approvals).toHaveLength(1);
    expect(pending.approvals[0]!.toolName).toBe('planning_apply');
    expect(json<{ run: { status: string } }>(await marc.get(`/runs/${runId}`)).run.status).toBe(
      'awaiting_approval',
    );

    // A second request cannot start while the run waits in this conversation.
    const concurrent = await marc.post(`/conversations/${conv.conversation.id}/messages`, {
      content: 'Autre demande',
    });
    expect(concurrent.statusCode).toBe(409);

    // Nobody else can validate an action requested on Marc's behalf.
    expect(
      (await julien.post(`/approvals/${pending.approvals[0]!.id}/decision`, { approved: true }))
        .statusCode,
    ).toBe(404);

    const decision = await marc.post(`/approvals/${pending.approvals[0]!.id}/decision`, {
      approved: true,
    });
    expect(decision.statusCode).toBe(200);
    await api.app.services.runs.idle();
    expect(
      (await marc.post(`/approvals/${pending.approvals[0]!.id}/decision`, { approved: false }))
        .statusCode,
    ).toBe(409);

    const detail = json<{
      run: { status: string };
      tools: Array<{ toolName: string; status: string }>;
    }>(await marc.get(`/runs/${runId}`));
    expect(detail.run.status).toBe('completed');
    expect(detail.tools.map((t) => `${t.toolName}:${t.status}`)).toEqual([
      'interventions_search:succeeded',
      'planning_propose:succeeded',
      'planning_apply:awaiting_approval',
      'planning_apply:succeeded',
    ]);

    const conversation = json<{ messages: Array<{ role: string; content: string }> }>(
      await marc.get(`/conversations/${conv.conversation.id}`),
    );
    expect(conversation.messages.map((m) => m.role)).toEqual(['user', 'assistant']);
    expect(conversation.messages[1]!.content).toContain('INT-2049');

    // Other users cannot see Marc's conversations or runs.
    expect((await julien.get(`/conversations/${conv.conversation.id}`)).statusCode).toBe(404);
    expect((await julien.get(`/runs/${runId}`)).statusCode).toBe(404);

    // The event stream replays persisted events and ends with the run.
    const sse = await api.app.inject({
      method: 'GET',
      url: `/runs/${runId}/events`,
      cookies: { mimorya_session: marcSession },
    });
    expect(sse.headers['content-type']).toContain('text/event-stream');
    expect(sse.body).toContain('event: approval_requested');
    expect(sse.body).toContain('event: run_finished');
    expect(sse.body.trim().endsWith('"status":"completed"}')).toBe(true);

    const replayFromEnd = await api.app.inject({
      method: 'GET',
      url: `/runs/${runId}/events`,
      headers: { 'last-event-id': '1000' },
      cookies: { mimorya_session: marcSession },
    });
    expect(replayFromEnd.body).toBe('');
  });
});
