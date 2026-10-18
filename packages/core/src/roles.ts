import type { PermissionPattern } from './authz/permissions';

export interface RoleTemplate {
  key: string;
  name: string;
  description: string;
  permissions: readonly PermissionPattern[];
}

/**
 * Platform role templates created for every new tenant. Tenants add their own
 * business roles (technician, team lead…) on top. Note that the technical
 * administrator manages the workspace but receives no business-data access.
 */
export const PLATFORM_ROLE_TEMPLATES: readonly RoleTemplate[] = [
  {
    key: 'member',
    name: 'Collaborateur',
    description: 'Accès de base : converser avec l’agent, consulter l’organisation et la mémoire.',
    permissions: [
      'agent:use',
      'org:read',
      'user:read',
      'memory:read',
      'mission:create',
      'mission:read',
      'skill:read',
      'connector:use',
    ],
  },
  {
    key: 'tenant_admin',
    name: 'Administrateur technique',
    description:
      'Gère l’espace entreprise (comptes, rôles, connecteurs, compétences). Ne donne pas accès aux données métier.',
    permissions: [
      'org:read',
      'org:manage',
      'user:read',
      'user:manage',
      'role:manage',
      'policy:manage',
      'connector:manage',
      'skill:manage',
      'settings:manage',
      'memory:manage',
      'audit:read',
      'usage:read',
    ],
  },
  {
    key: 'knowledge_manager',
    name: 'Gestionnaire des connaissances',
    description: 'Alimente et valide la mémoire partagée de son périmètre.',
    permissions: ['memory:read', 'memory:write', 'skill:read'],
  },
];
