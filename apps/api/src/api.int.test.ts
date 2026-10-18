/**
 * API integration tests against PostgreSQL, with a scripted model.
 * They exercise authentication, CSRF protection, authorization, the
 * conversation → run → approval → resume cycle, SSE replay and memory rights.
 */
import { ScriptedProvider, type ScriptedStep } from '@mimorya/agent/testing';
import { isoDate, mondayOf } from '@mimorya/connectors';
import { DEMO_USERS } from '@mimorya/core/demo';
import { resetTestDatabase } from '@mimorya/db/testing';
import type { FastifyInstance, LightMyRequestResponse } from 'fastify';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { buildApp } from './app';
import { hashPassword } from './auth/credentials';
import { loadConfig } from './config';

const PASSWORD = 'mot-de-passe-de-test';
const WEB_ORIGIN = 'http://localhost:3000';
const steps: ScriptedStep[] = [];
let app: FastifyInstance;
let appWithoutLlm: FastifyInstance;

const monday = mondayOf(new Date());
const WEEK = { from: isoDate(monday), to: isoDate(new Date(monday.getTime() + 4 * 86_400_000)) };
const NEXT_MONDAY = isoDate(new Date(monday.getTime() + 7 * 86_400_000));

async function build(withLlm: boolean) {
  const urls = {
    app:
      process.env.TEST_DATABASE_APP_URL ??
      'postgres://mimorya_app:mimorya_app@localhost:5432/mimorya_test',
  };
  const config = loadConfig({
    NODE_ENV: 'test',
    LOG_LEVEL: 'silent',
    DATABASE_APP_URL: urls.app,
    WEB_ORIGIN,
    LLM_PROVIDER: 'none',
  });
  return buildApp(config, { provider: withLlm ? new ScriptedProvider(steps) : null });
}

beforeAll(async () => {
  await resetTestDatabase({ passwordHash: await hashPassword(PASSWORD) });
  app = await build(true);
  appWithoutLlm = await build(false);
});

afterAll(async () => {
  await app?.close();
  await appWithoutLlm?.close();
});

const EMAILS: Record<string, string> = {
  [DEMO_USERS.marc]: 'marc.lefevre@neo-maintenance.example',
  [DEMO_USERS.julien]: 'julien.moreau@neo-maintenance.example',
  [DEMO_USERS.lucas]: 'lucas.petit@neo-maintenance.example',
  [DEMO_USERS.paul]: 'paul.renaud@neo-maintenance.example',
};

async function login(userId: string, target = app): Promise<string> {
  const res = await target.inject({
    method: 'POST',
    url: '/auth/login',
    headers: { 'x-mimorya-request': '1', origin: WEB_ORIGIN },
    payload: { email: EMAILS[userId], password: PASSWORD },
  });
  expect(res.statusCode).toBe(200);
  const cookie = res.cookies.find((c) => c.name === 'mimorya_session');
  expect(cookie?.httpOnly).toBe(true);
  return cookie!.value;
}

function as(session: string, target = app) {
  const headers = { 'x-mimorya-request': '1' };
  return {
    get: (url: string) =>
      target.inject({ method: 'GET', url, cookies: { mimorya_session: session } }),
    post: (url: string, payload?: object) =>
      target.inject({
        method: 'POST',
        url,
        payload: payload ?? {},
        headers,
        cookies: { mimorya_session: session },
      }),
    delete: (url: string) =>
      target.inject({ method: 'DELETE', url, headers, cookies: { mimorya_session: session } }),
  };
}

const json = <T = Record<string, unknown>>(res: LightMyRequestResponse) => res.json() as T;

describe('authentication', () => {
  it('rejects state-changing requests without the anti-CSRF header or from another origin', async () => {
    const noHeader = await app.inject({
      method: 'POST',
      url: '/auth/login',
      payload: { email: EMAILS[DEMO_USERS.marc], password: PASSWORD },
    });
    expect(noHeader.statusCode).toBe(403);
    const otherOrigin = await app.inject({
      method: 'POST',
      url: '/auth/login',
      headers: { 'x-mimorya-request': '1', origin: 'https://evil.example' },
      payload: { email: EMAILS[DEMO_USERS.marc], password: PASSWORD },
    });
    expect(otherOrigin.statusCode).toBe(403);
  });

  it('answers identically for unknown accounts and wrong passwords', async () => {
    const attempt = (email: string) =>
      app.inject({
        method: 'POST',
        url: '/auth/login',
        headers: { 'x-mimorya-request': '1' },
        payload: { email, password: 'incorrect' },
      });
    const wrong = await attempt(EMAILS[DEMO_USERS.marc]!);
    const unknown = await attempt('personne@inconnu.example');
    expect(wrong.statusCode).toBe(401);
    expect(unknown.statusCode).toBe(401);
    expect(wrong.json()).toEqual(unknown.json());
  });

  it('exposes effective permissions computed server-side', async () => {
    const marc = as(await login(DEMO_USERS.marc));
    const me = json<{ permissions: string[]; grants: string[] }>(await marc.get('/auth/me'));
    expect(me.permissions).toContain('intervention:read');
    expect(me.permissions).not.toContain('user:manage');
    expect(me.grants).toContain('Responsable d’équipe — Service technique et sous-unités');

    const anonymous = await app.inject({ method: 'GET', url: '/auth/me' });
    expect(anonymous.statusCode).toBe(401);
  });

  it('revokes the session on logout', async () => {
    const session = await login(DEMO_USERS.lucas);
    expect((await as(session).post('/auth/logout')).statusCode).toBe(200);
    expect((await as(session).get('/auth/me')).statusCode).toBe(401);
  });
});

describe('administration', () => {
  it('reserves administration screens to the matching permissions', async () => {
    const paul = as(await login(DEMO_USERS.paul));
    const marc = as(await login(DEMO_USERS.marc));
    expect((await paul.get('/admin/audit')).statusCode).toBe(200);
    expect((await marc.get('/admin/audit')).statusCode).toBe(403);
    const users = json<{ users: Array<{ grants?: string[] }> }>(await paul.get('/admin/users'));
    expect(users.users).toHaveLength(10);
    expect(users.users[0]!.grants).toBeDefined();
  });
});

describe('conversations and runs', () => {
  it('refuses to start a run when no model provider is configured', async () => {
    const marc = as(await login(DEMO_USERS.marc, appWithoutLlm), appWithoutLlm);
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

    steps.push(
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
    await app.services.runs.idle();

    const pending = json<{
      approvals: Array<{ id: string; toolName: string; actionSummary: string }>;
    }>(await marc.get('/approvals'));
    expect(pending.approvals).toHaveLength(1);
    expect(pending.approvals[0]!.toolName).toBe('planning_apply');
    expect(json<{ run: { status: string } }>(await marc.get(`/runs/${runId}`)).run.status).toBe(
      'awaiting_approval',
    );

    // Nobody else can validate an action requested on Marc's behalf.
    expect(
      (await julien.post(`/approvals/${pending.approvals[0]!.id}/decision`, { approved: true }))
        .statusCode,
    ).toBe(404);

    const decision = await marc.post(`/approvals/${pending.approvals[0]!.id}/decision`, {
      approved: true,
    });
    expect(decision.statusCode).toBe(200);
    await app.services.runs.idle();
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
    const sse = await app.inject({
      method: 'GET',
      url: `/runs/${runId}/events`,
      cookies: { mimorya_session: marcSession },
    });
    expect(sse.headers['content-type']).toContain('text/event-stream');
    expect(sse.body).toContain('event: approval_requested');
    expect(sse.body).toContain('event: run_finished');
    expect(sse.body.trim().endsWith('"status":"completed"}')).toBe(true);

    const replayFromEnd = await app.inject({
      method: 'GET',
      url: `/runs/${runId}/events`,
      headers: { 'last-event-id': '1000' },
      cookies: { mimorya_session: marcSession },
    });
    expect(replayFromEnd.body).toBe('');
  });
});

describe('memory rights', () => {
  it('lets users see and erase their own memory, never other people’s', async () => {
    const lucas = as(await login(DEMO_USERS.lucas));
    const marc = as(await login(DEMO_USERS.marc));
    const lucasItems = json<{ items: Array<{ id: string; title: string; canManage: boolean }> }>(
      await lucas.get('/memory'),
    ).items;
    expect(lucasItems.map((i) => i.title)).not.toContain('Format des synthèses');
    const own = lucasItems.find((i) => i.title === 'Prévenance des déplacements')!;
    expect(own.canManage).toBe(true);

    expect((await marc.delete(`/memory/${own.id}`)).statusCode).toBe(404);
    expect((await lucas.delete(`/memory/${own.id}`)).statusCode).toBe(200);
    const after = json<{ items: Array<{ id: string }> }>(await lucas.get('/memory')).items;
    expect(after.some((i) => i.id === own.id)).toBe(false);

    const shared = lucasItems.find((i) => i.title === 'Retours chantier équipe Nord')!;
    expect(shared.canManage).toBe(false);
    expect((await lucas.delete(`/memory/${shared.id}`)).statusCode).toBe(404);
  });
});
