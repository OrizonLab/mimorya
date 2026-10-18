/** Demo staff directory: technician names and scope-based selection. */
import { scopeIncludes, type DataScope } from '@mimorya/core';
import { DEMO_ROLES, DEMO_USER_LIST } from '@mimorya/core/demo';

function technician(id: string) {
  return DEMO_USER_LIST.find((u) => u.id === id);
}

export function technicianName(id: string): string {
  return technician(id)?.displayName ?? 'Inconnu';
}

/** Technicians whose schedule falls within a data scope. */
export function techniciansInScope(scope: DataScope): string[] {
  return DEMO_USER_LIST.filter(
    (u) =>
      u.assignments.some((a) => a.roleId === DEMO_ROLES.technician) &&
      scopeIncludes(scope, { ownerUserId: u.id, orgUnitId: u.primaryUnitId }),
  ).map((u) => u.id);
}
