/** Built-in tools: memory and skills. */
import {
  canWriteMemory,
  defineTool,
  memoryReadFilter,
  sensitivityRank,
  type AnyToolDefinition,
  type AuthorizationEngine,
  type OrgTree,
} from '@mimorya/core';
import { z } from 'zod';
import type { MemoryStore, SkillStore } from '../runtime/ports';

export interface PlatformToolDeps {
  engine: AuthorizationEngine;
  orgTree: OrgTree;
  memory: MemoryStore;
  skills: SkillStore;
}

const MEMORY_SPACES = {
  personal: 'Mémoire personnelle (visible par vous seul)',
  team: 'Mémoire de votre équipe principale',
  company: 'Mémoire de l’entreprise',
} as const;

export function createPlatformTools(deps: PlatformToolDeps): AnyToolDefinition[] {
  const memorySearch = defineTool({
    name: 'memory_search',
    title: 'Recherche dans la mémoire',
    description:
      'Recherche dans les mémoires accessibles à l’utilisateur (personnelle, équipe, entreprise, expérience) : procédures, préférences, décisions, faits utiles.',
    inputSchema: z.object({
      query: z.string().min(2).max(300).describe('Termes recherchés'),
      limit: z.number().int().min(1).max(10).optional(),
    }),
    risk: 'read',
    requiredPermissions: ['agent:use'],
    async execute(input, ctx) {
      const filter = memoryReadFilter(deps.engine, deps.orgTree, ctx.subject, ctx.actor);
      const hits = await deps.memory.search(filter, input.query, {
        limit: input.limit ?? 5,
        clearanceRank: sensitivityRank(ctx.subject.clearance ?? 'internal'),
      });
      await deps.memory.markUsed(hits.map((h) => h.id));
      return {
        content: hits.map((h) => ({
          id: h.id,
          espace: h.scope,
          type: h.kind,
          titre: h.title,
          contenu: h.content,
          maj: h.updatedAt.toISOString().slice(0, 10),
        })),
        summary: `${hits.length} élément(s) de mémoire trouvé(s)`,
        citations: hits.map((h) => ({ type: 'memory', id: h.id, label: h.title })),
      };
    },
  });

  const memoryRemember = defineTool({
    name: 'memory_remember',
    title: 'Mémorisation',
    description:
      'Enregistre une information utile pour les prochaines missions. Espace « personal » : préférences et informations de travail de l’utilisateur (enregistré immédiatement). Espaces « team » et « company » : connaissance partagée, enregistrée comme proposition à valider. N’enregistre jamais de secret, de mot de passe ni de donnée personnelle sensible.',
    inputSchema: z.object({
      space: z.enum(['personal', 'team', 'company']),
      kind: z.enum(['preference', 'fact', 'procedure', 'decision', 'lesson']),
      title: z.string().min(3).max(120),
      content: z.string().min(3).max(2000),
    }),
    risk: 'draft',
    requiredPermissions: ['agent:use'],
    describeAction: (input) => `Mémoriser « ${input.title} » (${MEMORY_SPACES[input.space]})`,
    async execute(input, ctx) {
      const primaryUnit = ctx.subject.orgUnitIds[0] ?? null;
      const target =
        input.space === 'personal'
          ? { scope: 'user' as const }
          : input.space === 'team'
            ? { scope: 'org_unit' as const, orgUnitId: primaryUnit }
            : { scope: 'tenant' as const };
      // Proposals to shared spaces are allowed for every member; they only
      // become visible once validated by someone holding memory:write.
      const direct = canWriteMemory(deps.engine, ctx.subject, target, ctx.actor);
      if (target.scope === 'org_unit' && !target.orgUnitId) {
        return { content: { erreur: 'Aucune équipe principale' }, isError: true };
      }
      const status = input.space === 'personal' && direct ? 'active' : 'proposed';
      const { id } = await deps.memory.create({
        scope: target.scope,
        ownerUserId: target.scope === 'user' ? ctx.subject.userId : null,
        orgUnitId: target.scope === 'org_unit' ? target.orgUnitId : null,
        kind: input.kind,
        title: input.title,
        content: input.content,
        status,
        source: { type: 'run', id: ctx.runId },
      });
      return {
        content: {
          id,
          statut: status === 'active' ? 'enregistré' : 'proposé (validation requise)',
          espace: MEMORY_SPACES[input.space],
        },
        summary:
          status === 'active'
            ? `Mémorisé : ${input.title}`
            : `Proposition de mémoire : ${input.title}`,
      };
    },
  });

  const skillRead = defineTool({
    name: 'skill_read',
    title: 'Lecture d’une compétence',
    description:
      'Charge le contenu complet d’une compétence (procédure validée par l’entreprise) listée dans les consignes. À utiliser avant d’exécuter une tâche couverte par une compétence.',
    inputSchema: z.object({ name: z.string().regex(/^[a-z0-9]+(-[a-z0-9]+)*$/) }),
    risk: 'read',
    requiredPermissions: ['skill:read'],
    async execute(input) {
      const skill = await deps.skills.getActive(input.name);
      if (!skill)
        return { content: { erreur: `Compétence « ${input.name} » introuvable` }, isError: true };
      return {
        content: skill.content,
        summary: `Compétence ${skill.name} v${skill.version}`,
        citations: [{ type: 'skill', id: `${skill.name}@${skill.version}` }],
      };
    },
  });

  return [memorySearch, memoryRemember, skillRead];
}
