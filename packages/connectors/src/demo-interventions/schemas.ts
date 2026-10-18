/** Input schemas and identifier patterns shared by the demo tools. */
import { z } from 'zod';

/** Identifiers of interventions cited in answers (grounding check). */
export const INTERVENTION_ID = /\bINT-\d{4}\b/g;

export const dateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/, 'Format attendu : AAAA-MM-JJ');
export const timeSchema = z.string().regex(/^([01]\d|2[0-3]):[0-5]\d$/, 'Format attendu : HH:MM');

/** One slot of a proposed follow-up planning. */
export const proposeItem = z.object({
  interventionId: z.string().regex(/^INT-\d{4}$/),
  technicianId: z.string(),
  date: dateSchema,
  startTime: timeSchema,
  durationHours: z.number().min(0.5).max(8),
  reason: z.string().min(3).max(300),
});
export type ProposedSlot = z.infer<typeof proposeItem>;
