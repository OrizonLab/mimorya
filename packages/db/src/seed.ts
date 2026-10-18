/**
 * Demo tenant provisioning (FICTIONAL data, see @mimorya/core/demo).
 * Idempotent: an existing demo tenant is left untouched unless `reset` is set.
 */
import {
  DEMO_ORG_UNITS,
  DEMO_POLICIES,
  DEMO_ROLE_DEFINITIONS,
  DEMO_TENANT,
  DEMO_UNITS,
  DEMO_USER_LIST,
  DEMO_USERS,
  demoId,
} from '@mimorya/core/demo';
import { eq } from 'drizzle-orm';
import { withTenant, type Database, type Transaction } from './client';
import {
  accessPolicies,
  connectorInstances,
  memoryItems,
  orgMemberships,
  orgUnits,
  roleAssignments,
  roles,
  skills,
  skillVersions,
  tenants,
  users,
} from './schema';

export interface SeedOptions {
  /** Argon2id hash applied to every demo account (null: accounts cannot log in). */
  passwordHash: string | null;
  reset?: boolean;
}

export const DEMO_CONNECTOR_ID = demoId(500);
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

export async function seedDemoTenant(
  db: Database,
  options: SeedOptions,
): Promise<'created' | 'skipped'> {
  return withTenant(db, DEMO_TENANT.id, async (tx) => {
    const existing = await tx
      .select({ id: tenants.id })
      .from(tenants)
      .where(eq(tenants.id, DEMO_TENANT.id));
    if (existing.length > 0) {
      if (!options.reset) return 'skipped';
      await tx.delete(tenants).where(eq(tenants.id, DEMO_TENANT.id));
    }
    await insertDemoData(tx, options);
    return 'created';
  });
}

async function insertDemoData(tx: Transaction, options: SeedOptions): Promise<void> {
  const tenantId = DEMO_TENANT.id;
  await tx.insert(tenants).values({
    id: tenantId,
    slug: DEMO_TENANT.slug,
    name: DEMO_TENANT.name,
    settings: { demo: true, defaultAutonomyLevel: 'execute_with_approval' },
  });

  await tx.insert(users).values(
    DEMO_USER_LIST.map((u) => ({
      id: u.id,
      tenantId,
      email: u.email,
      displayName: u.displayName,
      jobTitle: u.jobTitle,
      passwordHash: options.passwordHash,
      clearance: u.clearance,
      attributes: u.attributes,
    })),
  );

  // Parents are listed before children in the fixture.
  for (const unit of DEMO_ORG_UNITS) {
    await tx.insert(orgUnits).values({ ...unit, tenantId });
  }

  await tx.insert(orgMemberships).values(
    DEMO_USER_LIST.map((u) => ({
      tenantId,
      userId: u.id,
      orgUnitId: u.primaryUnitId,
      isPrimary: true,
    })),
  );

  await tx.insert(roles).values(
    DEMO_ROLE_DEFINITIONS.map((r) => ({
      id: r.id,
      tenantId,
      key: r.key,
      name: r.name,
      description: r.description,
      permissions: [...r.permissions],
      isSystem: r.isSystem,
    })),
  );

  await tx.insert(roleAssignments).values(
    DEMO_USER_LIST.flatMap((u) =>
      u.assignments.map((a) => ({
        tenantId,
        userId: u.id,
        roleId: a.roleId,
        scopeType: a.scope.type,
        scopeOrgUnitId: a.scope.type === 'org_unit' ? a.scope.orgUnitId : null,
        includeDescendants: a.scope.type === 'org_unit' ? a.scope.includeDescendants : true,
        grantedByUserId: DEMO_USERS.paul,
      })),
    ),
  );

  await tx.insert(accessPolicies).values(
    DEMO_POLICIES.map((p) => ({
      id: p.id,
      tenantId,
      description: p.description,
      effect: p.effect,
      permissions: [...p.permissions],
      condition: p.condition,
    })),
  );

  await tx.insert(connectorInstances).values({
    id: DEMO_CONNECTOR_ID,
    tenantId,
    type: 'demo_interventions',
    name: 'Interventions (démonstration)',
    config: { dataset: 'neo-maintenance', note: 'Données fictives' },
  });

  const source = { type: 'manual' as const, label: 'Données de démonstration' };
  await tx.insert(memoryItems).values([
    {
      tenantId,
      scope: 'org_unit',
      orgUnitId: DEMO_UNITS.teamNorth,
      kind: 'procedure',
      title: 'Retours chantier équipe Nord',
      content:
        'Les retours sur chantier de l’équipe Nord sont planifiés le jeudi après-midi. Prévoir 2 h par intervention avec pièce à remplacer.',
      source,
      createdByUserId: DEMO_USERS.julien,
    },
    {
      tenantId,
      scope: 'org_unit',
      orgUnitId: DEMO_UNITS.technical,
      kind: 'procedure',
      title: 'Clôture d’une intervention',
      content:
        'Une intervention n’est terminée que si le compte rendu est signé par le client et qu’aucune pièce n’est en attente.',
      source,
      createdByUserId: DEMO_USERS.marc,
    },
    {
      tenantId,
      scope: 'tenant',
      kind: 'fact',
      title: 'Horaires d’intervention',
      content:
        'Les interventions sont planifiées du lundi au vendredi de 8 h à 17 h. L’astreinte du week-end est organisée par la direction.',
      source,
      createdByUserId: DEMO_USERS.claire,
    },
    {
      tenantId,
      scope: 'user',
      ownerUserId: DEMO_USERS.marc,
      kind: 'preference',
      title: 'Format des synthèses',
      content: 'Marc préfère des synthèses courtes, sous forme de tableau trié par priorité.',
      source,
      createdByUserId: DEMO_USERS.marc,
    },
    {
      tenantId,
      scope: 'user',
      ownerUserId: DEMO_USERS.lucas,
      kind: 'preference',
      title: 'Prévenance des déplacements',
      content: 'Lucas souhaite être prévenu la veille de ses déplacements hors secteur.',
      source,
      createdByUserId: DEMO_USERS.lucas,
    },
  ]);

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
