/**
 * Demo memories, one per memory space, so the visibility rules can be seen at
 * work: team and department procedures, a company fact, personal preferences.
 */
import { DEMO_TENANT, DEMO_UNITS, DEMO_USERS } from '@mimorya/core/demo';
import type { Transaction } from '../client';
import { memoryItems } from '../schema';

export async function insertDemoMemories(tx: Transaction): Promise<void> {
  const tenantId = DEMO_TENANT.id;
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
}
