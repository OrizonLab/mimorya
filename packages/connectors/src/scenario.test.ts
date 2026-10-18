/**
 * Reference scenario (docs/AGENT_ENGINE.md §10), end to end with a scripted
 * model: the engine, the policy enforcement point, the demo connector, human
 * validation, verification and memory capitalisation work together.
 */
import { lastToolResults } from '@mimorya/agent/testing';
import { DEMO_UNITS, DEMO_USERS } from '@mimorya/core/demo';
import { describe, expect, it } from 'vitest';
import { MARC_PLAN, parseResult, setup, toolNames } from './scenario.fixtures';

describe('reference scenario — weekly intervention review', () => {
  it('runs the full mission for the technical manager with validation and capitalisation', async () => {
    let proposalId = '';
    const { start, runtime, ports, provider, store } = setup(DEMO_USERS.marc, [
      // 1. Permissions are reflected in the tools offered; the agent loads the skill and searches.
      (req) => {
        expect(toolNames(req)).toEqual([
          'interventions_get',
          'interventions_search',
          'memory_remember',
          'memory_search',
          'planning_apply',
          'planning_propose',
          'skill_read',
          'team_availability',
        ]);
        expect(req.system).toContain('synthese-hebdo-interventions');
        const turn = req.messages[0]!;
        expect(turn.role === 'user' && turn.content).toContain('Clôture d’une intervention');
        // Personal memory of another user never reaches the context.
        expect(turn.role === 'user' && turn.content).not.toContain('Lucas souhaite');
        return {
          text: 'Je charge la procédure et je recherche les interventions de la semaine.',
          toolCalls: [
            { name: 'skill_read', input: { name: 'synthese-hebdo-interventions' } },
            { name: 'interventions_search', input: { from: '2026-10-12', to: '2026-10-16' } },
          ],
        };
      },
      // 2-4. Data retrieval scoped to the department, details of the anomalies.
      (req) => {
        const [, search] = lastToolResults(req);
        const data = parseResult(search!.content) as {
          nombre: number;
          interventions: Array<{ id: string; anomalies: string[] }>;
        };
        expect(data.nombre).toBe(13);
        expect(data.interventions.find((i) => i.id === 'INT-2043')!.anomalies).toHaveLength(1);
        return {
          text: '',
          toolCalls: ['INT-2043', 'INT-2049', 'INT-2052'].map((id) => ({
            name: 'interventions_get',
            input: { id },
          })),
        };
      },
      // 5. Availability of the team next week.
      (req) => {
        expect(lastToolResults(req).every((r) => !r.isError)).toBe(true);
        return {
          text: '',
          toolCalls: [
            { name: 'team_availability', input: { from: '2026-10-19', to: '2026-10-23' } },
          ],
        };
      },
      // 7. A first proposal with a conflict is rejected by the connector's checks…
      (req) => {
        const availability = parseResult(lastToolResults(req)[0]!.content) as Array<{
          technicienId: string;
        }>;
        expect(availability).toHaveLength(6); // both teams' technicians, leads included
        return {
          text: '',
          toolCalls: [
            {
              name: 'planning_propose',
              input: {
                title: 'Suivi semaine 43',
                items: [{ ...MARC_PLAN[1]!, startTime: '09:00' }, ...MARC_PLAN.slice(2)],
              },
            },
          ],
        };
      },
      // …and corrected (8. coherence verified by the tool).
      (req) => {
        const result = lastToolResults(req)[0]!;
        expect(result.isError).toBe(true);
        expect(result.content).toContain('chevauche « INT-2061');
        return {
          text: '',
          toolCalls: [
            { name: 'planning_propose', input: { title: 'Suivi semaine 43', items: MARC_PLAN } },
          ],
        };
      },
      // 9-10. Applying requires validation; a lesson is proposed for the team memory.
      (req) => {
        const result = parseResult(lastToolResults(req)[0]!.content) as { propositionId: string };
        proposalId = result.propositionId;
        return {
          text: 'La proposition est prête. Je demande la validation pour l’appliquer.',
          toolCalls: [
            { name: 'planning_apply', input: { proposalId } },
            {
              name: 'memory_remember',
              input: {
                space: 'team',
                kind: 'lesson',
                title: 'Interventions « terminées » avec pièce en attente',
                content:
                  'Vérifier le compte rendu : une pièce en attente impose le statut « en cours » et un suivi.',
              },
            },
          ],
        };
      },
      // 6. Final synthesis, grounded in the records actually consulted.
      (req) => {
        const results = lastToolResults(req);
        expect(results).toHaveLength(2);
        expect(results.every((r) => !r.isError)).toBe(true);
        return {
          text: [
            '## Synthèse — semaine du 12 octobre',
            '- Non terminées : INT-2042, INT-2046, INT-2049, INT-2051.',
            '- Incohérences : INT-2043 (terminée avec pièce en attente), INT-2052 (non réalisée mais terminée).',
            '- Planning de suivi appliqué : 6 créneaux la semaine prochaine.',
          ].join('\n'),
        };
      },
    ]);

    const first = await start();
    expect(first.status).toBe('awaiting_approval');
    if (first.status !== 'awaiting_approval') return;
    expect(first.approvals.map((a) => a.toolName)).toEqual(['planning_apply']);
    expect(store.applied).toHaveLength(0);
    // The memory proposal ran (draft risk) in the same turn and awaits human validation.
    expect(ports.memories.find((m) => m.kind === 'lesson')).toMatchObject({
      scope: 'org_unit',
      orgUnitId: DEMO_UNITS.technical,
      status: 'proposed',
    });

    const final = await runtime.resume(first.checkpoint, {
      [first.approvals[0]!.approvalId]: { approved: true, decidedBy: DEMO_USERS.marc },
    });
    expect(final.status).toBe('completed');
    if (final.status !== 'completed') return;
    expect(final.verification.passed).toBe(true);
    expect(store.applied).toHaveLength(6);
    expect(ports.deliverableItems.get(proposalId)?.status).toBe('applied');
    expect(provider.remaining).toBe(0);

    // Traceability: every tool call is logged with its decision.
    expect(ports.toolRecords.map((r) => `${r.toolName}:${r.status}`)).toEqual([
      'skill_read:succeeded',
      'interventions_search:succeeded',
      'interventions_get:succeeded',
      'interventions_get:succeeded',
      'interventions_get:succeeded',
      'team_availability:succeeded',
      'planning_propose:failed',
      'planning_propose:succeeded',
      'planning_apply:awaiting_approval',
      'memory_remember:succeeded',
      'planning_apply:succeeded',
    ]);
  });

  it('gives a team lead only their team’s data for the same request', async () => {
    const { start } = setup(DEMO_USERS.julien, [
      {
        text: '',
        toolCalls: [
          { name: 'interventions_search', input: { from: '2026-10-12', to: '2026-10-16' } },
        ],
      },
      (req) => {
        const data = parseResult(lastToolResults(req)[0]!.content) as {
          interventions: Array<{ id: string }>;
        };
        expect(data.interventions.map((i) => i.id)).toEqual([
          'INT-2041',
          'INT-2042',
          'INT-2043',
          'INT-2044',
          'INT-2045',
          'INT-2046',
          'INT-2047',
        ]);
        return { text: 'Équipe Nord : INT-2042 et INT-2046 restent à terminer.' };
      },
    ]);
    expect((await start()).status).toBe('completed');
  });

  it('exposes no intervention data to the technical administrator', async () => {
    const { start, ports } = setup(DEMO_USERS.paul, [
      (req) => {
        expect(toolNames(req)).toEqual(['memory_remember', 'memory_search', 'skill_read']);
        return {
          text: '',
          toolCalls: [
            { name: 'interventions_search', input: { from: '2026-10-12', to: '2026-10-16' } },
          ],
        };
      },
      { text: 'Vous n’avez pas accès aux comptes rendus d’intervention.' },
    ]);
    expect((await start()).status).toBe('completed');
    expect(ports.toolRecords[0]).toMatchObject({
      toolName: 'interventions_search',
      status: 'denied',
    });
  });
});
