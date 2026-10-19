import type { PermissionView } from '@/lib/api';
import { PERMISSION_GROUPS } from '@/lib/labels';

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
