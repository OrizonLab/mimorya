/** The organisation as returned by `/admin/organization`. */
export interface OrgUnitView {
  id: string;
  parentId: string | null;
  name: string;
  type: string;
  leadUserId: string | null;
  members: string[];
}

export interface OrganizationView {
  units: OrgUnitView[];
  users: Array<{ id: string; displayName: string; jobTitle: string | null }>;
  /** Changing the tree requires `org:manage` on the whole company. */
  canManage: boolean;
}

/** Units in tree order, indented by depth, for selects. */
export function unitOptions(units: OrgUnitView[]): Array<{ id: string; label: string }> {
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
