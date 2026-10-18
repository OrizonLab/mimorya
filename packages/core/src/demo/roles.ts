/** FICTIONAL demo company "Néo Maintenance" — roles (platform templates plus business roles). */
import type { PermissionPattern } from '../authz/permissions';
import { PLATFORM_ROLE_TEMPLATES } from '../roles';
import { DEMO_ROLES } from './ids';

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
      'memory:read',
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
