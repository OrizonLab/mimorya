import type { ErasedAccount, IssuedInvitation } from '@/lib/api';

/**
 * What an administrator is shown once after creating, resetting or
 * erasing an account, or resetting its second factor (ADR-202).
 */
export type AccountNotice =
  | { kind: 'password'; who: string; password: string }
  | { kind: 'invitation'; who: string; email: string; invitation: IssuedInvitation }
  | { kind: 'resetLink'; who: string; email: string; expiresAt: string }
  | { kind: 'erased'; who: string; erased: ErasedAccount }
  | { kind: 'secondFactorReset'; who: string };
