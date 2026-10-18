/** The directory as returned by `/admin/users`. */
export interface AssignmentView {
  id: string;
  roleId: string;
  roleName: string;
  label: string;
  expiresAt: string | null;
}

export interface UserView {
  id: string;
  displayName: string;
  jobTitle: string | null;
  status: string;
  orgUnit: string | null;
  // Shown to administrators only.
  email?: string;
  lastLoginAt?: string | null;
  orgUnitId?: string | null;
  clearance?: string;
  assignments?: AssignmentView[];
}

export interface DirectoryView {
  users: UserView[];
  /** Account and role management, each requiring its permission company-wide. */
  canManage: { users: boolean; roles: boolean };
}

export interface RoleOption {
  id: string;
  name: string;
  description: string;
  permissions: string[];
}
