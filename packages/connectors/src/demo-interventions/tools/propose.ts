/** Tool `planning_propose`: validated draft of a follow-up planning (no calendar change). */
import { defineTool, type AnyToolDefinition } from '@mimorya/core';
import { z } from 'zod';
import { canRead } from '../access';
import { technicianName, techniciansInScope } from '../directory';
import { busySlots, isWeekday, toMinutes, WORK_END, WORK_START, type Busy } from '../schedule';
import { INTERVENTION_ID, proposeItem } from '../schemas';
import type { DemoInterventionsStore } from '../store';

export function createProposeTool(store: DemoInterventionsStore): AnyToolDefinition {
  return defineTool({
    name: 'planning_propose',
    title: 'Proposition de planning',
    description:
      'Crée une proposition de planning de suivi (brouillon, sans effet sur les agendas). Contrôle : intervention accessible, technicien du périmètre, jour ouvré, horaires 8 h – 17 h, absence de chevauchement. Renvoie l’identifiant de la proposition ou la liste des problèmes à corriger.',
    inputSchema: z.object({
      title: z.string().min(3).max(120),
      items: z.array(proposeItem).min(1).max(30),
    }),
    risk: 'draft',
    requiredPermissions: ['planning:propose', 'intervention:read'],
    citationPattern: INTERVENTION_ID,
    async execute(input, ctx) {
      const allowedTechnicians = new Set(techniciansInScope(ctx.authz.scope('planning:propose')));
      const problems: string[] = [];
      const planned: Busy[] = [];

      input.items.forEach((item, index) => {
        const label = `Créneau ${index + 1} (${item.interventionId})`;
        const intervention = store.interventions.find((i) => i.id === item.interventionId);
        if (!intervention || !canRead(ctx, intervention)) {
          problems.push(`${label} : intervention introuvable ou non accessible`);
        }
        if (!allowedTechnicians.has(item.technicianId)) {
          problems.push(`${label} : technicien hors de votre périmètre de planification`);
        }
        if (!isWeekday(item.date)) problems.push(`${label} : ${item.date} n’est pas un jour ouvré`);
        const start = toMinutes(item.startTime);
        const end = start + item.durationHours * 60;
        if (start < WORK_START || end > WORK_END) {
          problems.push(`${label} : en dehors des horaires 08:00-17:00`);
        }
        const conflicts = busySlots(store, item.technicianId, item.date).filter(
          (b) => start < b.end && end > b.start,
        );
        for (const c of conflicts) problems.push(`${label} : chevauche « ${c.label} »`);
        const key = `${item.technicianId}|${item.date}`;
        for (const other of planned.filter((p) => p.label === key)) {
          if (start < other.end && end > other.start) {
            problems.push(`${label} : chevauche un autre créneau de la proposition`);
          }
        }
        planned.push({ start, end, label: key });
      });

      if (problems.length > 0) {
        return {
          content: { erreur: 'Proposition non valide', problemes: problems },
          isError: true,
          summary: `${problems.length} problème(s) à corriger`,
        };
      }

      const deliverable = await ctx.deliverables.create({
        type: 'planning_proposal',
        title: input.title,
        content: {
          items: input.items.map((i) => ({ ...i, technicien: technicianName(i.technicianId) })),
        },
        orgUnitId: ctx.subject.orgUnitIds[0] ?? null,
      });
      return {
        content: {
          propositionId: deliverable.id,
          statut: 'brouillon',
          creneaux: input.items.length,
          note: 'Aucun agenda n’est modifié tant que la proposition n’est pas appliquée (planning_apply, soumis à validation).',
        },
        summary: `Proposition « ${input.title} » (${input.items.length} créneau(x))`,
        citations: [
          { type: 'deliverable', id: deliverable.id, label: input.title },
          ...input.items.map((i) => ({ type: 'intervention', id: i.interventionId })),
        ],
      };
    },
  });
}
