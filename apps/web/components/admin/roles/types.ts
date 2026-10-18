import { PERMISSION_GROUPS } from '@/lib/labels';

export interface PermissionView {
  permission: string;
  label: string;
  /** `false`: the agent can never exercise it. */
  delegable: boolean;
}

export interface RoleView {
  id: string;
  key: string;
  name: string;
  description: string;
  isSystem: boolean;
  permissions: string[];
  /** Assignments of the role, expired ones included. */
  assignments: number;
  heldByYou: boolean;
}

export interface RolesView {
  roles: RoleView[];
  catalog: PermissionView[];
}

/** The catalog grouped by resource, in catalog order, with French group names. */
export function groupCatalog(catalog: PermissionView[]): Array<[string, PermissionView[]]> {
  const groups = new Map<string, PermissionView[]>();
  for (const p of catalog) {
    const resource = p.permission.split(':')[0] ?? p.permission;
    const name = PERMISSION_GROUPS[resource] ?? resource;
    groups.set(name, [...(groups.get(name) ?? []), p]);
  }
  return [...groups.entries()];
}
