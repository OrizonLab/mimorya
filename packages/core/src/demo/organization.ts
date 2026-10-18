/**
 * FICTIONAL demo company used by the seed script, tests and the reference
 * scenario ("PME de services techniques"). All names and data are invented.
 * Identifiers are deterministic so that tests and seeds stay reproducible.
 */
import type { AbacPolicy, AssignmentScope } from '../authz/engine';
import type { OrgUnitType } from '../authz/org-tree';
import type { PermissionPattern } from '../authz/permissions';
import type { Sensitivity } from '../authz/sensitivity';
import { PLATFORM_ROLE_TEMPLATES } from '../roles';

export function demoId(n: number): string {
  return `00000000-0000-4000-8000-${n.toString().padStart(12, '0')}`;
}

export const DEMO_TENANT = {
  id: demoId(1),
  slug: 'neo-maintenance',
  name: 'Néo Maintenance (démo)',
} as const;

export const DEMO_UNITS = {
  company: demoId(100),
  direction: demoId(101),
  technical: demoId(102),
  teamNorth: demoId(103),
  teamSouth: demoId(104),
  admin: demoId(105),
} as const;

export const DEMO_USERS = {
  claire: demoId(200),
  marc: demoId(201),
  julien: demoId(202),
  lucas: demoId(203),
  emma: demoId(204),
  nadia: demoId(205),
  hugo: demoId(206),
  lea: demoId(207),
  isabelle: demoId(208),
  paul: demoId(209),
} as const;

export const DEMO_ROLES = {
  member: demoId(300),
  tenantAdmin: demoId(301),
  knowledgeManager: demoId(302),
  technician: demoId(310),
  teamLead: demoId(311),
  director: demoId(312),
  adminStaff: demoId(313),
} as const;

export interface DemoOrgUnit {
  id: string;
  parentId: string | null;
  name: string;
  type: OrgUnitType;
  leadUserId: string | null;
}

export const DEMO_ORG_UNITS: readonly DemoOrgUnit[] = [
  {
    id: DEMO_UNITS.company,
    parentId: null,
    name: 'Néo Maintenance',
    type: 'company',
    leadUserId: DEMO_USERS.claire,
  },
  {
    id: DEMO_UNITS.direction,
    parentId: DEMO_UNITS.company,
    name: 'Direction',
    type: 'direction',
    leadUserId: DEMO_USERS.claire,
  },
  {
    id: DEMO_UNITS.technical,
    parentId: DEMO_UNITS.company,
    name: 'Service technique',
    type: 'department',
    leadUserId: DEMO_USERS.marc,
  },
  {
    id: DEMO_UNITS.teamNorth,
    parentId: DEMO_UNITS.technical,
    name: 'Équipe Nord',
    type: 'team',
    leadUserId: DEMO_USERS.julien,
  },
  {
    id: DEMO_UNITS.teamSouth,
    parentId: DEMO_UNITS.technical,
    name: 'Équipe Sud',
    type: 'team',
    leadUserId: DEMO_USERS.nadia,
  },
  {
    id: DEMO_UNITS.admin,
    parentId: DEMO_UNITS.company,
    name: 'Service administratif',
    type: 'department',
    leadUserId: DEMO_USERS.isabelle,
  },
];

export interface DemoRole {
  id: string;
  key: string;
  name: string;
  description: string;
  permissions: readonly PermissionPattern[];
  isSystem: boolean;
}

function template(key: string) {
  const t = PLATFORM_ROLE_TEMPLATES.find((r) => r.key === key);
  if (!t) throw new Error(`Unknown role template ${key}`);
  return t;
}

export const DEMO_ROLE_DEFINITIONS: readonly DemoRole[] = [
  { id: DEMO_ROLES.member, ...template('member'), isSystem: true },
  { id: DEMO_ROLES.tenantAdmin, ...template('tenant_admin'), isSystem: true },
  { id: DEMO_ROLES.knowledgeManager, ...template('knowledge_manager'), isSystem: true },
  {
    id: DEMO_ROLES.technician,
    key: 'technician',
    name: 'Technicien',
    description: 'Consulte et complète ses propres interventions.',
    permissions: ['intervention:read', 'intervention:write'],
    isSystem: false,
  },
  {
    id: DEMO_ROLES.teamLead,
    key: 'team_lead',
    name: 'Responsable d’équipe',
    description: 'Pilote les interventions et le planning de son périmètre.',
    permissions: [
      'intervention:read',
      'planning:read',
      'planning:propose',
      'planning:apply',
      'memory:write',
      'approval:decide',
    ],
    isSystem: false,
  },
  {
    id: DEMO_ROLES.director,
    key: 'director',
    name: 'Direction',
    description: 'Vision globale de l’activité, sans gestion technique de l’espace.',
    permissions: [
      'intervention:read',
      'planning:read',
      'planning:propose',
      'memory:write',
      'usage:read',
    ],
    isSystem: false,
  },
  {
    id: DEMO_ROLES.adminStaff,
    key: 'admin_staff',
    name: 'Assistant administratif',
    description: 'Consulte les interventions terminées pour la facturation.',
    permissions: ['intervention:read'],
    isSystem: false,
  },
];

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

/** Example ABAC policy: administrative staff only sees completed interventions (billing). */
export const DEMO_POLICIES: readonly AbacPolicy[] = [
  {
    id: demoId(400),
    description: 'Le service administratif ne consulte que les interventions terminées',
    effect: 'deny',
    permissions: ['intervention:read'],
    condition: {
      all: [
        {
          op: 'eq',
          left: { attr: 'subject.attributes.department' },
          right: { value: 'administratif' },
        },
        { op: 'eq', left: { attr: 'resource.type' }, right: { value: 'intervention' } },
        { op: 'neq', left: { attr: 'resource.attributes.status' }, right: { value: 'completed' } },
      ],
    },
  },
];
