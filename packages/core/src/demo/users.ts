/** FICTIONAL demo company "Néo Maintenance" — staff and their role assignments. */
import type { AssignmentScope } from '../authz/types';
import type { Sensitivity } from '../authz/sensitivity';
import { DEMO_ROLES, DEMO_UNITS, DEMO_USERS } from './ids';

export interface DemoUser {
  id: string;
  email: string;
  displayName: string;
  jobTitle: string;
  primaryUnitId: string;
  clearance: Sensitivity;
  attributes: Record<string, unknown>;
  assignments: ReadonlyArray<{ roleId: string; scope: AssignmentScope }>;
}

const SELF: AssignmentScope = { type: 'self' };
const TENANT: AssignmentScope = { type: 'tenant' };
const subtree = (orgUnitId: string): AssignmentScope => ({
  type: 'org_unit',
  orgUnitId,
  includeDescendants: true,
});

function user(
  id: string,
  first: string,
  last: string,
  jobTitle: string,
  primaryUnitId: string,
  assignments: DemoUser['assignments'],
  clearance: Sensitivity = 'internal',
  attributes: Record<string, unknown> = {},
): DemoUser {
  const email = `${first}.${last}`.toLowerCase().normalize('NFD').replace(/[̀-ͯ]/g, '');
  return {
    id,
    email: `${email}@neo-maintenance.example`,
    displayName: `${first} ${last}`,
    jobTitle,
    primaryUnitId,
    clearance,
    attributes,
    assignments: [{ roleId: DEMO_ROLES.member, scope: SELF }, ...assignments],
  };
}

export const DEMO_USER_LIST: readonly DemoUser[] = [
  user(
    DEMO_USERS.claire,
    'Claire',
    'Martin',
    'Directrice générale',
    DEMO_UNITS.direction,
    [{ roleId: DEMO_ROLES.director, scope: TENANT }],
    'confidential',
  ),
  user(
    DEMO_USERS.marc,
    'Marc',
    'Lefèvre',
    'Responsable technique',
    DEMO_UNITS.technical,
    [{ roleId: DEMO_ROLES.teamLead, scope: subtree(DEMO_UNITS.technical) }],
    'confidential',
  ),
  user(
    DEMO_USERS.julien,
    'Julien',
    'Moreau',
    'Chef d’équipe Nord',
    DEMO_UNITS.teamNorth,
    [
      { roleId: DEMO_ROLES.teamLead, scope: subtree(DEMO_UNITS.teamNorth) },
      { roleId: DEMO_ROLES.technician, scope: SELF },
    ],
    'confidential',
  ),
  user(DEMO_USERS.lucas, 'Lucas', 'Petit', 'Technicien', DEMO_UNITS.teamNorth, [
    { roleId: DEMO_ROLES.technician, scope: SELF },
  ]),
  user(DEMO_USERS.emma, 'Emma', 'Roux', 'Technicienne', DEMO_UNITS.teamNorth, [
    { roleId: DEMO_ROLES.technician, scope: SELF },
  ]),
  user(
    DEMO_USERS.nadia,
    'Nadia',
    'Benali',
    'Cheffe d’équipe Sud',
    DEMO_UNITS.teamSouth,
    [
      { roleId: DEMO_ROLES.teamLead, scope: subtree(DEMO_UNITS.teamSouth) },
      { roleId: DEMO_ROLES.technician, scope: SELF },
    ],
    'confidential',
  ),
  user(DEMO_USERS.hugo, 'Hugo', 'Garnier', 'Technicien', DEMO_UNITS.teamSouth, [
    { roleId: DEMO_ROLES.technician, scope: SELF },
  ]),
  user(DEMO_USERS.lea, 'Léa', 'Fontaine', 'Technicienne', DEMO_UNITS.teamSouth, [
    { roleId: DEMO_ROLES.technician, scope: SELF },
  ]),
  user(
    DEMO_USERS.isabelle,
    'Isabelle',
    'Laurent',
    'Assistante administrative',
    DEMO_UNITS.admin,
    [{ roleId: DEMO_ROLES.adminStaff, scope: TENANT }],
    'internal',
    { department: 'administratif' },
  ),
  user(DEMO_USERS.paul, 'Paul', 'Renaud', 'Administrateur technique', DEMO_UNITS.admin, [
    { roleId: DEMO_ROLES.tenantAdmin, scope: TENANT },
  ]),
];
