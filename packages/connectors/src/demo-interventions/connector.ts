/**
 * Demo connector "Interventions" — FICTIONAL data, in-memory state.
 * It shows how a connector enforces the caller's data scope: every record is
 * checked with `ctx.authz` before being returned, and existence of
 * inaccessible records is never revealed.
 */
import {
  defineTool,
  scopeIncludes,
  type AnyToolDefinition,
  type DataScope,
  type ToolContext,
} from '@mimorya/core';
import { DEMO_ROLES, DEMO_USER_LIST } from '@mimorya/core/demo';
import { z } from 'zod';
import {
  buildDemoDataset,
  detectAnomalies,
  isoDate,
  type Absence,
  type Intervention,
} from './dataset';
import type { ConnectorDefinition } from '../types';

const INTERVENTION_ID = /\bINT-\d{4}\b/g;
const WORK_START = 8 * 60;
const WORK_END = 17 * 60;

export interface ScheduledSlot {
  technicianId: string;
  date: string;
  startTime: string;
  durationHours: number;
  label: string;
}

export class DemoInterventionsStore {
  readonly interventions: Intervention[];
  readonly absences: Absence[];
  readonly applied: ScheduledSlot[] = [];

  constructor(readonly reference: Date = new Date()) {
    const data = buildDemoDataset(reference);
    this.interventions = data.interventions;
    this.absences = data.absences;
  }
}

function technician(id: string) {
  return DEMO_USER_LIST.find((u) => u.id === id);
}

function technicianName(id: string): string {
  return technician(id)?.displayName ?? 'Inconnu';
}

function asResource(i: Intervention) {
  return {
    type: 'intervention',
    id: i.id,
    ownerUserId: i.technicianId,
    orgUnitId: i.teamUnitId,
    sensitivity: i.sensitivity,
    attributes: { status: i.status },
  };
}

function canRead(ctx: ToolContext, i: Intervention): boolean {
  return ctx.authz.check('intervention:read', asResource(i)).allowed;
}

function toMinutes(time: string): number {
  const [h, m] = time.split(':').map(Number);
  return (h ?? 0) * 60 + (m ?? 0);
}

function toTime(minutes: number): string {
  return `${String(Math.floor(minutes / 60)).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}`;
}

/** Technicians whose schedule falls within a data scope. */
function techniciansInScope(scope: DataScope): string[] {
  return DEMO_USER_LIST.filter(
    (u) =>
      u.assignments.some((a) => a.roleId === DEMO_ROLES.technician) &&
      scopeIncludes(scope, { ownerUserId: u.id, orgUnitId: u.primaryUnitId }),
  ).map((u) => u.id);
}

interface Busy {
  start: number;
  end: number;
  label: string;
}

function busySlots(store: DemoInterventionsStore, technicianId: string, date: string): Busy[] {
  const fromInterventions = store.interventions
    .filter((i) => i.technicianId === technicianId && i.date === date && i.status !== 'cancelled')
    .map((i) => ({
      start: toMinutes(i.startTime),
      end: toMinutes(i.startTime) + i.durationHours * 60,
      label: `${i.id} ${i.type}`,
    }));
  const fromAbsences = store.absences
    .filter((a) => a.technicianId === technicianId && a.date === date)
    .map((a) => ({ start: toMinutes(a.startTime), end: toMinutes(a.endTime), label: a.reason }));
  const fromApplied = store.applied
    .filter((s) => s.technicianId === technicianId && s.date === date)
    .map((s) => ({
      start: toMinutes(s.startTime),
      end: toMinutes(s.startTime) + s.durationHours * 60,
      label: s.label,
    }));
  return [...fromInterventions, ...fromAbsences, ...fromApplied].sort((a, b) => a.start - b.start);
}

function isWeekday(date: string): boolean {
  const day = new Date(`${date}T00:00:00Z`).getUTCDay();
  return day >= 1 && day <= 5;
}

function datesBetween(from: string, to: string): string[] {
  const dates: string[] = [];
  const cursor = new Date(`${from}T00:00:00Z`);
  const end = new Date(`${to}T00:00:00Z`);
  while (cursor <= end && dates.length < 31) {
    dates.push(isoDate(cursor));
    cursor.setUTCDate(cursor.getUTCDate() + 1);
  }
  return dates;
}

const dateSchema = z.string().regex(/^\d{4}-\d{2}-\d{2}$/, 'Format attendu : AAAA-MM-JJ');
const timeSchema = z.string().regex(/^([01]\d|2[0-3]):[0-5]\d$/, 'Format attendu : HH:MM');

export function createDemoInterventionTools(store: DemoInterventionsStore): AnyToolDefinition[] {
  const search = defineTool({
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

  const get = defineTool({
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

  const availability = defineTool({
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

  const proposeItem = z.object({
    interventionId: z.string().regex(/^INT-\d{4}$/),
    technicianId: z.string(),
    date: dateSchema,
    startTime: timeSchema,
    durationHours: z.number().min(0.5).max(8),
    reason: z.string().min(3).max(300),
  });

  const propose = defineTool({
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

  const apply = defineTool({
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

  return [search, get, availability, propose, apply];
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
