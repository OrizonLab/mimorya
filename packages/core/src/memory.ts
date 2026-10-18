/**
 * Memory visibility rules. Memory items live in one of four spaces:
 *   - user:       personal working memory, visible to its owner only
 *   - org_unit:   team/department knowledge
 *   - tenant:     company-wide knowledge
 *   - experience: validated lessons and procedures used by the agent
 *
 * Shared spaces are readable by members of the unit (and of its sub-units:
 * a team member sees the department's knowledge) and by anyone whose
 * `memory:read` scope covers the unit. Writing to a shared space requires
 * `memory:write` on that scope. Personal memory is never shared through the
 * hierarchy: a manager cannot read a collaborator's personal memory.
 */
import type { AuthorizationEngine } from './authz/engine';
import type { Actor, Subject } from './authz/types';
import type { OrgTree } from './authz/org-tree';

export const MEMORY_SCOPES = ['user', 'org_unit', 'tenant', 'experience'] as const;
export type MemoryScope = (typeof MEMORY_SCOPES)[number];

export const MEMORY_KINDS = [
  'preference',
  'fact',
  'procedure',
  'decision',
  'lesson',
  'episode',
] as const;
export type MemoryKind = (typeof MEMORY_KINDS)[number];

export interface MemoryReadFilter {
  /** Personal memory of this user is visible. */
  ownerUserId: string;
  /** Shared org-unit memories visible; `'all'` when the scope is tenant-wide. */
  orgUnitIds: ReadonlySet<string> | 'all';
  /** Company-wide and experience memories visible. */
  shared: boolean;
}

export function memoryReadFilter(
  engine: AuthorizationEngine,
  orgTree: OrgTree,
  subject: Subject,
  actor?: Actor,
): MemoryReadFilter {
  const scope = engine.dataScope(subject, 'memory:read', actor);
  if (scope.kind === 'none') {
    return { ownerUserId: subject.userId, orgUnitIds: new Set(), shared: false };
  }
  if (scope.kind === 'all') {
    return { ownerUserId: subject.userId, orgUnitIds: 'all', shared: true };
  }
  const units = new Set<string>(scope.orgUnitIds);
  for (const unitId of subject.orgUnitIds) {
    units.add(unitId);
    for (const ancestor of orgTree.ancestors(unitId)) units.add(ancestor.id);
  }
  return { ownerUserId: subject.userId, orgUnitIds: units, shared: true };
}

export interface MemoryTarget {
  scope: MemoryScope;
  orgUnitId?: string | null;
}

/** Whether the subject (or an agent acting for them) may write into a memory space. */
export function canWriteMemory(
  engine: AuthorizationEngine,
  subject: Subject,
  target: MemoryTarget,
  actor?: Actor,
): boolean {
  if (target.scope === 'user') return engine.has(subject, 'agent:use', actor);
  const scope = engine.dataScope(subject, 'memory:write', actor);
  if (scope.kind === 'all') return true;
  if (scope.kind === 'none') return false;
  if (target.scope === 'org_unit') {
    return !!target.orgUnitId && scope.orgUnitIds.has(target.orgUnitId);
  }
  // tenant-wide and experience memories require a tenant-wide write scope.
  return false;
}
