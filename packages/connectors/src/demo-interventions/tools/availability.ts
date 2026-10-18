/** Tool `team_availability`: busy and free slots of the technicians in scope. */
import { defineTool, type AnyToolDefinition } from '@mimorya/core';
import { z } from 'zod';
import { technicianName, techniciansInScope } from '../directory';
import { busySlots, datesBetween, isWeekday, toTime, WORK_END, WORK_START } from '../schedule';
import { dateSchema } from '../schemas';
import type { DemoInterventionsStore } from '../store';

export function createAvailabilityTool(store: DemoInterventionsStore): AnyToolDefinition {
  return defineTool({
    name: 'team_availability',
    title: 'Disponibilités de l’équipe',
    description:
      'Renvoie, pour chaque technicien du périmètre de l’utilisateur, les créneaux occupés et libres (jours ouvrés, 8 h – 17 h) sur une période (31 jours maximum).',
    inputSchema: z.object({ from: dateSchema, to: dateSchema }),
    risk: 'read',
    requiredPermissions: ['planning:read'],
    async execute(input, ctx) {
      const technicians = techniciansInScope(ctx.authz.scope('planning:read'));
      const days = datesBetween(input.from, input.to).filter(isWeekday);
      const result = technicians.map((technicianId) => ({
        technicien: technicianName(technicianId),
        technicienId: technicianId,
        jours: days.map((date) => {
          const busy = busySlots(store, technicianId, date);
          const free: string[] = [];
          let cursor = WORK_START;
          for (const slot of busy) {
            if (slot.start > cursor) free.push(`${toTime(cursor)}-${toTime(slot.start)}`);
            cursor = Math.max(cursor, slot.end);
          }
          if (cursor < WORK_END) free.push(`${toTime(cursor)}-${toTime(WORK_END)}`);
          return {
            date,
            occupe: busy.map((b) => `${toTime(b.start)}-${toTime(b.end)} ${b.label}`),
            libre: free,
          };
        }),
      }));
      return { content: result, summary: `Disponibilités de ${technicians.length} technicien(s)` };
    },
  });
}
