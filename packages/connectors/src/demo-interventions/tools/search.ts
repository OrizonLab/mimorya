/** Tool `interventions_search`: interventions of a period, filtered by the caller's scope. */
import { defineTool, type AnyToolDefinition } from '@mimorya/core';
import { z } from 'zod';
import { canRead } from '../access';
import { detectAnomalies } from '../anomalies';
import { technicianName } from '../directory';
import { dateSchema, INTERVENTION_ID } from '../schemas';
import type { DemoInterventionsStore } from '../store';

export function createSearchTool(store: DemoInterventionsStore): AnyToolDefinition {
  return defineTool({
    name: 'interventions_search',
    title: 'Recherche d’interventions',
    description:
      'Recherche les interventions (comptes rendus) accessibles à l’utilisateur sur une période. Renvoie un résumé par intervention, avec les anomalies détectées automatiquement. Utiliser interventions_get pour le compte rendu complet.',
    inputSchema: z.object({
      from: dateSchema.describe('Date de début incluse'),
      to: dateSchema.describe('Date de fin incluse'),
      status: z.array(z.enum(['planned', 'in_progress', 'completed', 'cancelled'])).optional(),
      technicianId: z.string().optional(),
    }),
    risk: 'read',
    requiredPermissions: ['intervention:read'],
    citationPattern: INTERVENTION_ID,
    async execute(input, ctx) {
      const results = store.interventions
        .filter((i) => i.date >= input.from && i.date <= input.to)
        .filter((i) => !input.status || input.status.includes(i.status))
        .filter((i) => !input.technicianId || i.technicianId === input.technicianId)
        .filter((i) => canRead(ctx, i))
        .sort((a, b) => (a.date + a.startTime).localeCompare(b.date + b.startTime));
      return {
        content: {
          periode: { du: input.from, au: input.to },
          nombre: results.length,
          interventions: results.map((i) => ({
            id: i.id,
            date: i.date,
            technicien: technicianName(i.technicianId),
            technicienId: i.technicianId,
            type: i.type,
            client: i.client,
            statut: i.status,
            piece_en_attente: i.partsPending,
            anomalies: detectAnomalies(i),
          })),
        },
        summary: `${results.length} intervention(s) trouvée(s)`,
        citations: results.map((i) => ({
          type: 'intervention',
          id: i.id,
          label: `${i.type} — ${i.client}`,
        })),
      };
    },
  });
}
