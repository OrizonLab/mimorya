/** Tool `planning_apply`: applies a draft planning to the calendars (requires validation). */
import { defineTool, type AnyToolDefinition } from '@mimorya/core';
import { z } from 'zod';
import type { proposeItem } from '../schemas';
import type { DemoInterventionsStore } from '../store';

export function createApplyTool(store: DemoInterventionsStore): AnyToolDefinition {
  return defineTool({
    name: 'planning_apply',
    title: 'Application d’un planning',
    description:
      'Applique une proposition de planning aux agendas des techniciens. Action soumise à la validation de l’utilisateur.',
    inputSchema: z.object({ proposalId: z.string().min(1) }),
    risk: 'write',
    requiredPermissions: ['planning:apply'],
    describeAction: (input) =>
      `Appliquer la proposition de planning ${input.proposalId} aux agendas de l’équipe`,
    async execute(input, ctx) {
      const proposal = await ctx.deliverables.get(input.proposalId);
      if (
        !proposal ||
        proposal.type !== 'planning_proposal' ||
        proposal.ownerUserId !== ctx.subject.userId
      ) {
        return { content: { erreur: 'Proposition introuvable' }, isError: true };
      }
      if (proposal.status !== 'draft' && proposal.status !== 'approved') {
        return { content: { erreur: `Proposition déjà ${proposal.status}` }, isError: true };
      }
      const items = (proposal.content as { items: Array<z.infer<typeof proposeItem>> }).items;
      for (const item of items) {
        store.applied.push({
          technicianId: item.technicianId,
          date: item.date,
          startTime: item.startTime,
          durationHours: item.durationHours,
          label: `Suivi ${item.interventionId}`,
        });
      }
      await ctx.deliverables.setStatus(proposal.id, 'applied');
      return {
        content: { propositionId: proposal.id, statut: 'appliquée', creneaux: items.length },
        summary: `Planning appliqué (${items.length} créneau(x))`,
      };
    },
  });
}
