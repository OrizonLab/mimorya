import type { IssuedInvitation } from '@/lib/api';

/** What an administrator is shown once after creating or resetting an account. */
export type AccountNotice =
  | { kind: 'password'; who: string; password: string }
  | { kind: 'invitation'; who: string; email: string; invitation: IssuedInvitation }
  | { kind: 'resetLink'; who: string; email: string; expiresAt: string };
