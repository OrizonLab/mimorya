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
  /** Read by access policies as `subject.attributes.*`. */
  attributes?: Record<string, unknown>;
  /** When the pending invitation link of an `invited` account expires. */
  invitationExpiresAt?: string | null;
  assignments?: AssignmentView[];
}

/** An invitation link just issued: e-mailed, or to transmit. */
export interface IssuedInvitation {
  expiresAt: string;
  emailed: boolean;
  /** Shown once, when no e-mail went out. */
  link: string | null;
  /** Why no e-mail went out. */
  problem: string | null;
}

/** What an administrator is shown once after creating or resetting an account. */
export type AccountNotice =
  | { kind: 'password'; who: string; password: string }
  | { kind: 'invitation'; who: string; email: string; invitation: IssuedInvitation };

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
