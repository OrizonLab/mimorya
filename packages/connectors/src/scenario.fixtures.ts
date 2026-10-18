/** Fixtures of the reference scenario: a scripted run over the demo connector. */
import {
  AgentRuntime,
  buildSystemPrompt,
  buildUserTurn,
  createPlatformTools,
  DEFAULT_BUDGET,
  ModelRouter,
  PriceTable,
  ToolRegistry,
  type LlmRequest,
} from '@mimorya/agent';
import { InMemoryPorts, ScriptedProvider, type ScriptedStep } from '@mimorya/agent/testing';
import { memoryReadFilter, sensitivityRank } from '@mimorya/core';
import {
  DEMO_UNITS,
  DEMO_USERS,
  demoAuthorizationEngine,
  demoOrgTree,
  demoSubject,
} from '@mimorya/core/demo';
import { createDemoInterventionTools, DemoInterventionsStore } from './demo-interventions';

const engine = demoAuthorizationEngine();
const orgTree = demoOrgTree();
const REFERENCE = new Date('2026-10-14T10:00:00Z');
export const REQUEST =
  'Analyse les comptes rendus des interventions de cette semaine, identifie les interventions non terminées, prépare une synthèse et propose un planning de suivi pour mon équipe.';

export function setup(userId: string, steps: ScriptedStep[]) {
  const subject = demoSubject(userId);
  const runId = `run-${userId.slice(-3)}`;
  const ports = new InMemoryPorts(
    { userId, runId },
    {
      skills: [
        {
          name: 'synthese-hebdo-interventions',
          description: 'Synthèse hebdomadaire des interventions et planning de suivi',
          content: '# Synthèse hebdomadaire\n1. Rechercher… 6. Présenter…',
        },
      ],
      memories: [
        {
          scope: 'org_unit',
          orgUnitId: DEMO_UNITS.technical,
          kind: 'procedure',
          title: 'Clôture d’une intervention',
          content:
            'Une intervention n’est terminée que si le compte rendu est signé et qu’aucune pièce n’est en attente.',
        },
        {
          scope: 'user',
          ownerUserId: DEMO_USERS.lucas,
          kind: 'preference',
          title: 'Déplacements',
          content: 'Lucas souhaite être prévenu la veille de ses interventions.',
        },
      ],
    },
  );
  const store = new DemoInterventionsStore(REFERENCE);
  const registry = new ToolRegistry([
    ...createPlatformTools({ engine, orgTree, memory: ports.memory, skills: ports.skills }),
    ...createDemoInterventionTools(store),
  ]);
  const provider = new ScriptedProvider(steps);
  const runtime = new AgentRuntime(
    {
      router: new ModelRouter({ agent: provider }),
      engine,
      registry,
      ports,
      prices: new PriceTable(),
      retryDelayMs: 1,
    },
    {
      runId,
      subject,
      actor: { type: 'agent', runId },
      objective: REQUEST,
      autonomy: { level: 'execute_with_approval' },
      budget: DEFAULT_BUDGET,
      successCriteria: [],
      verification: { llmJudge: false, maxCorrectionAttempts: 2 },
    },
  );

  async function start() {
    const memories = await ports.memory.search(
      memoryReadFilter(engine, orgTree, subject),
      'interventions terminée compte rendu',
      { limit: 5, clearanceRank: sensitivityRank(subject.clearance ?? 'internal') },
    );
    const system = buildSystemPrompt(
      {
        assistantName: 'Mimorya',
        tenantName: 'Néo Maintenance (démo)',
        displayName: 'Marc Lefèvre',
        jobTitle: 'Responsable technique',
        orgUnitPath: orgTree.pathLabel(DEMO_UNITS.technical),
        roles: ['Responsable d’équipe — Service technique et sous-unités'],
      },
      await ports.skills.listActive(),
      REFERENCE,
    );
    return runtime.start({ system, history: [], userTurn: buildUserTurn(REQUEST, memories) });
  }
  return { runtime, ports, provider, store, start };
}

export function toolNames(request: LlmRequest): string[] {
  return request.tools.map((t) => t.name).sort();
}

export function parseResult(content: string): unknown {
  const json = content.slice(content.indexOf('\n') + 1, content.lastIndexOf('\n'));
  return JSON.parse(json);
}

/** Valid follow-up planning for the technical manager (week after the reference date). */
export const MARC_PLAN = [
  {
    interventionId: 'INT-2042',
    technicianId: DEMO_USERS.emma,
    date: '2026-10-19',
    startTime: '09:00',
    durationHours: 2,
    reason: 'Remplacement de la vanne Schrader',
  },
  {
    interventionId: 'INT-2043',
    technicianId: DEMO_USERS.lucas,
    date: '2026-10-19',
    startTime: '13:00',
    durationHours: 2,
    reason: 'Pose du circulateur Grundfos',
  },
  {
    interventionId: 'INT-2046',
    technicianId: DEMO_USERS.lucas,
    date: '2026-10-20',
    startTime: '09:00',
    durationHours: 2,
    reason: 'Diagnostic et compte rendu manquant',
  },
  {
    interventionId: 'INT-2049',
    technicianId: DEMO_USERS.lea,
    date: '2026-10-19',
    startTime: '10:00',
    durationHours: 2,
    reason: 'Nouveau passage avec le gardien',
  },
  {
    interventionId: 'INT-2051',
    technicianId: DEMO_USERS.nadia,
    date: '2026-10-20',
    startTime: '09:00',
    durationHours: 3,
    reason: 'Remplacement de la pompe après validation du devis',
  },
  {
    interventionId: 'INT-2052',
    technicianId: DEMO_USERS.lea,
    date: '2026-10-20',
    startTime: '14:00',
    durationHours: 2,
    reason: 'Entretien à reprogrammer (client absent)',
  },
];
