import type { OrgUnitView } from '@/lib/api';

/** Units in tree order, indented by depth, for selects. */
export function unitOptions(
  units: ReadonlyArray<Pick<OrgUnitView, 'id' | 'parentId' | 'name'>>,
): Array<{ id: string; label: string }> {
  const options: Array<{ id: string; label: string }> = [];
  const visit = (parentId: string | null, depth: number) => {
    for (const unit of units.filter((u) => u.parentId === parentId)) {
      options.push({ id: unit.id, label: `${'  '.repeat(depth)}${unit.name}` });
      visit(unit.id, depth + 1);
    }
  };
  visit(null, 0);
  return options;
}
