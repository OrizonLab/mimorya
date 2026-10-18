/**
 * Permissions are `domain:action` strings. Roles bundle permissions; role
 * assignments give them a scope (see engine.ts). A pattern may use `*` as the
 * action (`memory:*`) or as the whole permission (`*`) — patterns are only
 * accepted in role definitions and policies, never in checks.
 */
export type Permission = `${string}:${string}`;
export type PermissionPattern = Permission | '*';

const PERMISSION_RE = /^[a-z][a-z0-9_.-]*:[a-z][a-z0-9_.-]*$/;
const PATTERN_RE = /^(\*|[a-z][a-z0-9_.-]*:(\*|[a-z][a-z0-9_.-]*))$/;

export function isPermission(value: string): value is Permission {
  return PERMISSION_RE.test(value);
}

export function isPermissionPattern(value: string): value is PermissionPattern {
  return PATTERN_RE.test(value);
}

export function matchesPermission(pattern: PermissionPattern, permission: Permission): boolean {
  if (pattern === '*') return true;
  if (pattern === permission) return true;
  const [pDomain, pAction] = pattern.split(':');
  const [domain] = permission.split(':');
  return pAction === '*' && pDomain === domain;
}

export interface PermissionInfo {
  permission: Permission;
  label: string;
  /** Permissions an agent may never exercise on behalf of a user (see NON_DELEGABLE). */
  delegable: boolean;
}

/**
 * Platform permission catalog. Connectors register their own domain permissions
 * (e.g. `intervention:read`) through the connector registry.
 */
export const PLATFORM_PERMISSIONS: readonly PermissionInfo[] = [
  { permission: 'agent:use', label: "Converser avec l'agent", delegable: true },
  { permission: 'org:read', label: "Consulter l'organisation", delegable: true },
  { permission: 'org:manage', label: "Gérer l'organisation", delegable: false },
  { permission: 'user:read', label: 'Consulter l’annuaire', delegable: true },
  { permission: 'user:manage', label: 'Gérer les comptes', delegable: false },
  { permission: 'role:manage', label: 'Gérer les rôles et habilitations', delegable: false },
  { permission: 'policy:manage', label: 'Gérer les politiques d’accès', delegable: false },
  { permission: 'memory:read', label: 'Lire la mémoire partagée', delegable: true },
  { permission: 'memory:write', label: 'Écrire dans la mémoire partagée', delegable: true },
  { permission: 'memory:manage', label: 'Administrer la mémoire', delegable: false },
  { permission: 'mission:create', label: 'Confier des missions', delegable: true },
  { permission: 'mission:read', label: 'Suivre les missions', delegable: true },
  { permission: 'approval:decide', label: 'Valider les actions proposées', delegable: false },
  { permission: 'skill:read', label: 'Consulter les compétences', delegable: true },
  { permission: 'skill:manage', label: 'Gérer et activer les compétences', delegable: false },
  { permission: 'connector:use', label: 'Utiliser les connecteurs', delegable: true },
  { permission: 'connector:manage', label: 'Configurer les connecteurs', delegable: false },
  { permission: 'audit:read', label: 'Consulter le journal d’audit', delegable: false },
  { permission: 'settings:manage', label: 'Paramétrer l’espace entreprise', delegable: false },
  { permission: 'usage:read', label: 'Consulter la consommation', delegable: true },
];

/**
 * Hard ceiling for agents: whatever the user's rights, an agent run can never
 * exercise these. This is what prevents a prompt injection or a self-improved
 * skill from granting privileges or altering security rules.
 */
export const NON_DELEGABLE_PERMISSIONS: ReadonlySet<Permission> = new Set(
  PLATFORM_PERMISSIONS.filter((p) => !p.delegable).map((p) => p.permission),
);
