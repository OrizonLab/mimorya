/**
 * Demo connector "Interventions" — FICTIONAL data, in-memory state.
 * It shows how a connector enforces the caller's data scope: every record is
 * checked with `ctx.authz` before being returned, and existence of
 * inaccessible records is never revealed.
 */
import type { AnyToolDefinition } from '@mimorya/core';
import type { ConnectorDefinition } from '../types';
import { DemoInterventionsStore } from './store';
import { createApplyTool } from './tools/apply';
import { createAvailabilityTool } from './tools/availability';
import { createGetTool } from './tools/get';
import { createProposeTool } from './tools/propose';
import { createSearchTool } from './tools/search';

export function createDemoInterventionTools(store: DemoInterventionsStore): AnyToolDefinition[] {
  return [
    createSearchTool(store),
    createGetTool(store),
    createAvailabilityTool(store),
    createProposeTool(store),
    createApplyTool(store),
  ];
}

export const demoInterventionsConnector: ConnectorDefinition<{ referenceDate?: string }> = {
  type: 'demo_interventions',
  name: 'Interventions (démonstration)',
  description:
    'Connecteur de démonstration : comptes rendus d’intervention et planning d’une PME fictive. Données inventées, état en mémoire.',
  demo: true,
  permissions: [
    { permission: 'intervention:read', label: 'Consulter les interventions', delegable: true },
    { permission: 'intervention:write', label: 'Modifier les interventions', delegable: true },
    { permission: 'planning:read', label: 'Consulter les plannings', delegable: true },
    { permission: 'planning:propose', label: 'Proposer des plannings', delegable: true },
    { permission: 'planning:apply', label: 'Appliquer des plannings', delegable: true },
  ],
  instantiate(config) {
    const store = new DemoInterventionsStore(
      config.referenceDate ? new Date(config.referenceDate) : new Date(),
    );
    return { tools: createDemoInterventionTools(store) };
  },
};
