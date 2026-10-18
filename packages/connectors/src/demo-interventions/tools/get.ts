/** Tool `interventions_get`: full report of one accessible intervention. */
import { defineTool, type AnyToolDefinition } from '@mimorya/core';
import { z } from 'zod';
import { canRead } from '../access';
import { detectAnomalies } from '../anomalies';
import { technicianName } from '../directory';
import { INTERVENTION_ID } from '../schemas';
import type { DemoInterventionsStore } from '../store';

export function createGetTool(store: DemoInterventionsStore): AnyToolDefinition {
  return defineTool({
    name: 'interventions_get',
    title: 'Détail d’une intervention',
    description: 'Renvoie le compte rendu complet d’une intervention accessible à l’utilisateur.',
    inputSchema: z.object({ id: z.string().regex(/^INT-\d{4}$/) }),
    risk: 'read',
    requiredPermissions: ['intervention:read'],
    citationPattern: INTERVENTION_ID,
    async execute(input, ctx) {
      const i = store.interventions.find((x) => x.id === input.id);
      // Same answer whether the record does not exist or is out of scope.
      if (!i || !canRead(ctx, i)) {
        return {
          content: { erreur: `Intervention ${input.id} introuvable ou non accessible` },
          isError: true,
        };
      }
      return {
        content: {
          id: i.id,
          date: i.date,
          heure: i.startTime,
          duree_heures: i.durationHours,
          technicien: technicianName(i.technicianId),
          technicienId: i.technicianId,
          type: i.type,
          client: i.client,
          ville: i.city,
          statut: i.status,
          compte_rendu: i.report || '(vide)',
          piece_en_attente: i.partsPending,
          signature_client: i.clientSignature,
          anomalies: detectAnomalies(i),
        },
        summary: `${i.id} — ${i.type}`,
        citations: [{ type: 'intervention', id: i.id, label: `${i.type} — ${i.client}` }],
      };
    },
  });
}
