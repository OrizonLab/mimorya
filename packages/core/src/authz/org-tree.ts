/**
 * In-memory view of a tenant's organisation (company, sites, departments,
 * teams…) with unlimited depth. TPE/PME trees are small (tens to hundreds of
 * units), so we load them per request/cache and compute subtrees in process.
 */
export const ORG_UNIT_TYPES = [
  'company',
  'site',
  'direction',
  'department',
  'service',
  'team',
] as const;
export type OrgUnitType = (typeof ORG_UNIT_TYPES)[number];

export interface OrgUnit {
  id: string;
  parentId: string | null;
  name: string;
  type: OrgUnitType;
  /** Organisational lead. Informational only: it grants no access by itself. */
  leadUserId?: string | null;
}

export class OrgTree {
  private readonly units = new Map<string, OrgUnit>();
  private readonly children = new Map<string, string[]>();

  constructor(units: readonly OrgUnit[]) {
    for (const unit of units) {
      if (this.units.has(unit.id)) throw new Error(`Duplicate org unit ${unit.id}`);
      this.units.set(unit.id, unit);
    }
    for (const unit of units) {
      if (unit.parentId === null) continue;
      if (!this.units.has(unit.parentId)) {
        throw new Error(`Org unit ${unit.id} references unknown parent ${unit.parentId}`);
      }
      const siblings = this.children.get(unit.parentId) ?? [];
      siblings.push(unit.id);
      this.children.set(unit.parentId, siblings);
    }
    for (const unit of units) this.assertNoCycle(unit.id);
  }

  get(id: string): OrgUnit | undefined {
    return this.units.get(id);
  }

  all(): OrgUnit[] {
    return [...this.units.values()];
  }

  roots(): OrgUnit[] {
    return this.all().filter((u) => u.parentId === null);
  }

  childrenOf(id: string): OrgUnit[] {
    return (this.children.get(id) ?? []).map((cid) => this.units.get(cid)!);
  }

  /** The unit itself and all of its descendants. */
  subtree(id: string): Set<string> {
    const result = new Set<string>();
    if (!this.units.has(id)) return result;
    const stack = [id];
    while (stack.length > 0) {
      const current = stack.pop()!;
      result.add(current);
      for (const child of this.children.get(current) ?? []) stack.push(child);
    }
    return result;
  }

  /** Ancestors from the direct parent up to the root. */
  ancestors(id: string): OrgUnit[] {
    const result: OrgUnit[] = [];
    let current = this.units.get(id);
    while (current?.parentId) {
      const parent = this.units.get(current.parentId);
      if (!parent) break;
      result.push(parent);
      current = parent;
    }
    return result;
  }

  /** Human readable path, e.g. "Entreprise › Service technique › Équipe Nord". */
  pathLabel(id: string): string {
    const unit = this.units.get(id);
    if (!unit) return '';
    return [...this.ancestors(id).reverse(), unit].map((u) => u.name).join(' › ');
  }

  isInSubtree(rootId: string, candidateId: string): boolean {
    if (rootId === candidateId) return this.units.has(rootId);
    return this.ancestors(candidateId).some((a) => a.id === rootId);
  }

  private assertNoCycle(startId: string): void {
    const seen = new Set<string>();
    let current = this.units.get(startId);
    while (current) {
      if (seen.has(current.id)) throw new Error(`Cycle detected in org tree at ${current.id}`);
      seen.add(current.id);
      current = current.parentId ? this.units.get(current.parentId) : undefined;
    }
  }
}
