/** Demo skill: the weekly interventions report, active and approved (v1). */
import { DEMO_TENANT, DEMO_USERS, demoId } from '@mimorya/core/demo';
import { eq } from 'drizzle-orm';
import type { Transaction } from '../client';
import { skills, skillVersions } from '../schema';

export const DEMO_SKILL_ID = demoId(600);

const WEEKLY_REPORT_SKILL = `---
name: synthese-hebdo-interventions
description: Produire la synthèse hebdomadaire des interventions d'un périmètre et proposer un planning de suivi des interventions non terminées.
license: Proprietary
metadata:
  mimorya:
    category: operations
    tools: [interventions_search, interventions_get, team_availability, planning_propose]
---

# Synthèse hebdomadaire des interventions

1. Rechercher les interventions de la période demandée (par défaut : semaine en cours).
2. Pour chaque intervention non terminée ou signalée en anomalie, consulter le détail.
3. Classer : non terminées, terminées avec anomalie, terminées.
4. Pour chaque intervention non terminée, identifier la cause (pièce, accès client,
   compte rendu incomplet) et l'action de suivi.
5. Consulter les disponibilités de l'équipe et proposer un créneau par suivi
   (sans double réservation, en respectant les procédures d'équipe connues).
6. Présenter : tableau de synthèse (identifiants INT-xxxx), anomalies, planning proposé,
   actions nécessitant validation.
`;

export async function insertDemoSkills(tx: Transaction): Promise<void> {
  const tenantId = DEMO_TENANT.id;
  await tx.insert(skills).values({
    id: DEMO_SKILL_ID,
    tenantId,
    name: 'synthese-hebdo-interventions',
    description:
      'Produire la synthèse hebdomadaire des interventions d’un périmètre et proposer un planning de suivi.',
    status: 'active',
    createdByUserId: DEMO_USERS.marc,
  });
  const [version] = await tx
    .insert(skillVersions)
    .values({
      tenantId,
      skillId: DEMO_SKILL_ID,
      version: 1,
      content: WEEKLY_REPORT_SKILL,
      frontmatter: { name: 'synthese-hebdo-interventions' },
      changelog: 'Version initiale (démonstration)',
      status: 'active',
      createdByUserId: DEMO_USERS.marc,
      approvedByUserId: DEMO_USERS.paul,
      approvedAt: new Date(),
    })
    .returning({ id: skillVersions.id });
  await tx.update(skills).set({ activeVersionId: version!.id }).where(eq(skills.id, DEMO_SKILL_ID));
}
