/** Administration sections and the permission each one requires. */
export const ADMIN_SECTIONS = [
  { key: 'org', label: 'Organisation', permission: 'org:read' },
  { key: 'users', label: 'Utilisateurs', permission: 'user:read' },
  { key: 'roles', label: 'Rôles', permission: 'role:manage' },
  { key: 'policies', label: 'Politiques d’accès', permission: 'policy:manage' },
  { key: 'approvalRouting', label: 'Circuit de validation', permission: 'policy:manage' },
  { key: 'knowledgeShares', label: 'Partage de connaissances', permission: 'policy:manage' },
  { key: 'audit', label: 'Journal d’audit', permission: 'audit:read' },
  { key: 'usage', label: 'Consommation', permission: 'usage:read' },
  { key: 'llm', label: 'Modèle d’IA', permission: 'settings:manage' },
  { key: 'identityProviders', label: 'Connexion', permission: 'settings:manage' },
  { key: 'connectors', label: 'Connecteurs', permission: 'connector:manage' },
  { key: 'skills', label: 'Compétences', permission: 'skill:manage' },
] as const;

export type AdminSectionKey = (typeof ADMIN_SECTIONS)[number]['key'];

/** Sections the user may open; the API enforces the same permissions. */
export function visibleAdminSections(permissions: readonly string[]) {
  return ADMIN_SECTIONS.filter((s) => permissions.includes(s.permission));
}
