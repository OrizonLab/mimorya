/**
 * Administration sections and the permissions that open each one, any of
 * them: the audit log's, to read it or to export it (ADR-182).
 */
export const ADMIN_SECTIONS = [
  { key: 'org', label: 'Organisation', permissions: ['org:read'] },
  { key: 'users', label: 'Utilisateurs', permissions: ['user:read'] },
  { key: 'roles', label: 'Rôles', permissions: ['role:manage'] },
  { key: 'policies', label: 'Politiques d’accès', permissions: ['policy:manage'] },
  { key: 'approvalRouting', label: 'Circuit de validation', permissions: ['policy:manage'] },
  { key: 'knowledgeShares', label: 'Partage de connaissances', permissions: ['policy:manage'] },
  { key: 'audit', label: 'Journal d’audit', permissions: ['audit:read', 'audit:export'] },
  { key: 'usage', label: 'Consommation', permissions: ['usage:read'] },
  { key: 'llm', label: 'Modèle d’IA', permissions: ['settings:manage'] },
  { key: 'identityProviders', label: 'Connexion', permissions: ['settings:manage'] },
  { key: 'connectors', label: 'Connecteurs', permissions: ['connector:manage'] },
  { key: 'skills', label: 'Compétences', permissions: ['skill:manage'] },
] as const;

export type AdminSectionKey = (typeof ADMIN_SECTIONS)[number]['key'];

/** Sections the user may open; the API enforces the same permissions. */
export function visibleAdminSections(permissions: readonly string[]) {
  return ADMIN_SECTIONS.filter((s) =>
    (s.permissions as readonly string[]).some((p) => permissions.includes(p)),
  );
}
