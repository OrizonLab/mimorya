/** Runtime: tool visibility, policy enforcement and untrusted tool output. */
import { DEMO_USERS, demoSubject } from '@mimorya/core/demo';
import { describe, expect, it } from 'vitest';
import { lastToolResults } from '../testing';
import { setup } from './runtime.fixtures';

describe('AgentRuntime — tools', () => {
  it('only shows the model the tools the user may use, never non-delegable ones', async () => {
    const paul = setup([{ text: 'Bonjour' }], { subject: demoSubject(DEMO_USERS.paul) });
    await paul.start();
    const names = paul.provider.requests[0]!.tools.map((t) => t.name);
    expect(names).toEqual(['doc_read']);

    const julien = setup([{ text: 'Bonjour' }]);
    await julien.start();
    expect(julien.provider.requests[0]!.tools.map((t) => t.name).sort()).toEqual([
      'doc_read',
      'jobs_search',
      'planning_apply',
    ]);
  });

  it('runs read tools with the user’s data scope and returns grounded answers', async () => {
    const { start, ports, provider } = setup([
      { text: 'Je cherche.', toolCalls: [{ name: 'jobs_search', input: { query: 'semaine' } }] },
      (req) => {
        const result = lastToolResults(req)[0]!;
        expect(result.content).toContain('fiabilite="non_fiable"');
        expect(result.content).toContain('INT-0001');
        expect(result.content).not.toContain('INT-0002');
        return { text: 'Une intervention : INT-0001.' };
      },
    ]);
    const outcome = await start();
    expect(outcome.status).toBe('completed');
    if (outcome.status !== 'completed') return;
    expect(outcome.verification.passed).toBe(true);
    expect(outcome.answer).toBe('Une intervention : INT-0001.');
    expect(provider.remaining).toBe(0);
    expect(ports.toolRecords.map((r) => [r.toolName, r.status])).toEqual([
      ['jobs_search', 'succeeded'],
    ]);
    expect(ports.auditEntries.map((a) => a.action)).toEqual(['agent.tool.execute']);
    expect(ports.usageRecords).toHaveLength(2);
  });

  it('denies calls to tools the user cannot use, even if the model insists', async () => {
    const { start, ports } = setup([
      { text: '', toolCalls: [{ name: 'users_disable', input: { userId: DEMO_USERS.lucas } }] },
      (req) => {
        expect(lastToolResults(req)[0]!.content).toMatch(/Refusé par la plateforme/);
        return { text: 'Je ne peux pas désactiver de compte.' };
      },
    ]);
    const outcome = await start();
    expect(outcome.status).toBe('completed');
    expect(ports.toolRecords[0]).toMatchObject({ toolName: 'users_disable', status: 'denied' });
    expect(ports.auditEntries[0]).toMatchObject({ action: 'agent.tool.denied', outcome: 'denied' });
  });

  it('neutralises prompt injection carried by tool results', async () => {
    const { start, ports } = setup(
      [
        { text: '', toolCalls: [{ name: 'doc_read', input: {} }] },
        (req) => {
          const content = lastToolResults(req)[0]!.content;
          // The payload cannot close the untrusted envelope.
          expect(content.match(/<\/donnees_outil>/g)).toHaveLength(1);
          // A compromised model following the injected instruction is still stopped by the PEP.
          return { text: '', toolCalls: [{ name: 'users_disable', input: { userId: 'all' } }] };
        },
        { text: 'Le document contenait une instruction suspecte que je n’ai pas suivie.' },
      ],
      { subject: demoSubject(DEMO_USERS.paul) },
    );
    const outcome = await start();
    expect(outcome.status).toBe('completed');
    expect(ports.toolRecords.find((r) => r.toolName === 'users_disable')?.status).toBe('denied');
  });

  it('rejects invalid tool inputs and lets the model correct them', async () => {
    const { start, ports } = setup([
      { text: '', toolCalls: [{ name: 'jobs_search', input: { query: 42 } }] },
      (req) => {
        expect(lastToolResults(req)[0]!.content).toMatch(/Paramètres invalides/);
        return { text: '', toolCalls: [{ name: 'jobs_search', input: { query: 'ok' } }] };
      },
      { text: 'INT-0001 est concernée.' },
    ]);
    expect((await start()).status).toBe('completed');
    expect(ports.toolRecords.map((r) => r.status)).toEqual(['denied', 'succeeded']);
  });
});
