/** Built-in skill tool: loads the full content of an active company skill. */
import { defineTool, type AnyToolDefinition } from '@mimorya/core';
import { z } from 'zod';
import type { SkillStore } from '../../runtime/ports';

export function createSkillTools(deps: { skills: SkillStore }): AnyToolDefinition[] {
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

  return [skillRead];
}
