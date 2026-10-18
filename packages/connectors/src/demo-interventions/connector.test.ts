import { ToolExecutor, ToolRegistry } from '@mimorya/agent';
import { InMemoryPorts } from '@mimorya/agent/testing';
import { DEMO_USERS, demoAuthorizationEngine, demoSubject } from '@mimorya/core/demo';
import { describe, expect, it } from 'vitest';
import { createDemoInterventionTools } from './connector';
import { DemoInterventionsStore } from './store';
import { mondayOf } from './dates';

const engine = demoAuthorizationEngine();
const REFERENCE = new Date('2026-10-14T10:00:00Z'); // Wednesday
const WEEK = { from: '2026-10-12', to: '2026-10-16' };

function harness(userId: string) {
  const store = new DemoInterventionsStore(REFERENCE);
  const registry = new ToolRegistry(createDemoInterventionTools(store));
  const ports = new InMemoryPorts({ userId, runId: 'run-1' });
  const executor = new ToolExecutor({
    engine,
    registry,
    subject: demoSubject(userId),
    actor: { type: 'agent', runId: 'run-1' },
    autonomy: { level: 'execute_with_approval' },
    runId: 'run-1',
    deliverables: ports.deliverables,
  });
  async function call(name: string, input: unknown) {
    const evaluation = executor.evaluate({ id: 'c', name, input });
    if (evaluation.kind !== 'allow') return { evaluation, result: undefined };
    const { result } = await executor.execute(evaluation.tool, evaluation.input);
    return { evaluation, result };
  }
  return { store, call, ports };
}

function ids(result: unknown): string[] {
  return (
    result as { content: { interventions: Array<{ id: string }> } }
  ).content.interventions.map((i) => i.id);
}

describe('demo interventions connector', () => {
  it('builds the week around the reference date', () => {
    expect(mondayOf(REFERENCE).toISOString().slice(0, 10)).toBe('2026-10-12');
  });

  it('scopes search results to each user’s permissions', async () => {
    const lucas = await harness(DEMO_USERS.lucas).call('interventions_search', WEEK);
    expect(ids(lucas.result)).toEqual(['INT-2041', 'INT-2043', 'INT-2046']);

    const julien = await harness(DEMO_USERS.julien).call('interventions_search', WEEK);
    expect(ids(julien.result)).toEqual([
      'INT-2041',
      'INT-2042',
      'INT-2043',
      'INT-2044',
      'INT-2045',
      'INT-2046',
      'INT-2047',
    ]);

    const marc = await harness(DEMO_USERS.marc).call('interventions_search', WEEK);
    expect(ids(marc.result)).toHaveLength(13);
    expect(ids(marc.result)).toContain('INT-2051'); // confidential, Marc has clearance

    const claire = await harness(DEMO_USERS.claire).call('interventions_search', WEEK);
    expect(ids(claire.result)).toHaveLength(13);
  });

  it('applies ABAC and clearance rules per record', async () => {
    const isabelle = await harness(DEMO_USERS.isabelle).call('interventions_search', WEEK);
    // Administrative staff: completed interventions only, and no confidential record.
    expect(ids(isabelle.result)).toEqual([
      'INT-2048',
      'INT-2041',
      'INT-2043',
      'INT-2044',
      'INT-2045',
      'INT-2050',
      'INT-2052',
    ]);
    const statuses = (
      isabelle.result as { content: { interventions: Array<{ statut: string }> } }
    ).content.interventions.map((i) => i.statut);
    expect(new Set(statuses)).toEqual(new Set(['completed']));
  });

  it('hides the tools entirely from users without the permission', async () => {
    const paul = await harness(DEMO_USERS.paul).call('interventions_search', WEEK);
    expect(paul.evaluation).toMatchObject({
      kind: 'deny',
      reason: 'Permission intervention:read requise',
    });
  });

  it('does not reveal whether an inaccessible record exists', async () => {
    const lucas = harness(DEMO_USERS.lucas);
    const other = await lucas.call('interventions_get', { id: 'INT-2048' });
    const missing = await lucas.call('interventions_get', { id: 'INT-9999' });
    expect(other.result?.content).toEqual({
      erreur: 'Intervention INT-2048 introuvable ou non accessible',
    });
    expect(missing.result?.content).toEqual({
      erreur: 'Intervention INT-9999 introuvable ou non accessible',
    });
  });

  it('flags inconsistent reports', async () => {
    const { result } = await harness(DEMO_USERS.marc).call('interventions_get', { id: 'INT-2043' });
    expect((result?.content as { anomalies: string[] }).anomalies).toEqual([
      'Statut « terminée » alors qu’une pièce est en attente (Circulateur Grundfos)',
    ]);
  });

  it('validates planning proposals and creates a draft deliverable', async () => {
    const { call, ports } = harness(DEMO_USERS.julien);
    const invalid = await call('planning_propose', {
      title: 'Suivi',
      items: [
        // Lucas is busy on Monday morning (INT-2061)
        {
          interventionId: 'INT-2043',
          technicianId: DEMO_USERS.lucas,
          date: '2026-10-19',
          startTime: '09:00',
          durationHours: 2,
          reason: 'Pose du circulateur',
        },
        // Hugo belongs to the other team
        {
          interventionId: 'INT-2042',
          technicianId: DEMO_USERS.hugo,
          date: '2026-10-20',
          startTime: '09:00',
          durationHours: 2,
          reason: 'Vanne',
        },
        // Saturday
        {
          interventionId: 'INT-2046',
          technicianId: DEMO_USERS.emma,
          date: '2026-10-24',
          startTime: '09:00',
          durationHours: 2,
          reason: 'Compte rendu',
        },
      ],
    });
    expect(invalid.result?.isError).toBe(true);
    const problems = (invalid.result?.content as { problemes: string[] }).problemes;
    expect(problems).toEqual([
      'Créneau 1 (INT-2043) : chevauche « INT-2061 Entretien chaufferie »',
      'Créneau 2 (INT-2042) : technicien hors de votre périmètre de planification',
      'Créneau 3 (INT-2046) : 2026-10-24 n’est pas un jour ouvré',
    ]);

    const valid = await call('planning_propose', {
      title: 'Suivi semaine 43',
      items: [
        {
          interventionId: 'INT-2043',
          technicianId: DEMO_USERS.lucas,
          date: '2026-10-19',
          startTime: '13:00',
          durationHours: 2,
          reason: 'Pose du circulateur',
        },
      ],
    });
    expect(valid.result?.isError).toBeFalsy();
    expect([...ports.deliverableItems.values()]).toMatchObject([
      { type: 'planning_proposal', status: 'draft' },
    ]);
  });

  it('requires approval to apply a planning, then updates availability', async () => {
    const { call, ports, store } = harness(DEMO_USERS.julien);
    const proposal = await call('planning_propose', {
      title: 'Suivi',
      items: [
        {
          interventionId: 'INT-2046',
          technicianId: DEMO_USERS.lucas,
          date: '2026-10-20',
          startTime: '08:00',
          durationHours: 2,
          reason: 'Compléter le compte rendu',
        },
      ],
    });
    const proposalId = (proposal.result?.content as { propositionId: string }).propositionId;
    const apply = await call('planning_apply', { proposalId });
    expect(apply.evaluation.kind).toBe('require_approval');
    expect(store.applied).toHaveLength(0);

    // Executing as the approved action:
    if (apply.evaluation.kind !== 'require_approval') return;
    const executor = new ToolExecutor({
      engine,
      registry: new ToolRegistry([apply.evaluation.tool]),
      subject: demoSubject(DEMO_USERS.julien),
      actor: { type: 'agent', runId: 'run-1' },
      autonomy: { level: 'execute_with_approval' },
      runId: 'run-1',
      deliverables: ports.deliverables,
    });
    const { result } = await executor.execute(apply.evaluation.tool, apply.evaluation.input);
    expect(result.isError).toBeFalsy();
    expect(store.applied).toHaveLength(1);
    expect(ports.deliverableItems.get(proposalId)?.status).toBe('applied');
  });
});
