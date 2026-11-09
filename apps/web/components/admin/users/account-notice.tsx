'use client';

import { ErasedNotice } from './erased-notice';
import { InvitationNotice } from './invitation-notice';
import { ResetLinkNotice } from './reset-link-notice';
import { TemporaryPassword } from './temporary-password';
import type { AccountNotice } from './types';

/**
 * The temporary password, invitation or reset link an administrator just
 * issued, or what went with the account they just erased.
 */
export function AccountNoticeCard({
  notice,
  onClose,
}: {
  notice: AccountNotice;
  onClose: () => void;
}) {
  if (notice.kind === 'password') {
    return <TemporaryPassword who={notice.who} password={notice.password} onClose={onClose} />;
  }
  if (notice.kind === 'erased') {
    return <ErasedNotice who={notice.who} erased={notice.erased} onClose={onClose} />;
  }
  if (notice.kind === 'resetLink') {
    return (
      <ResetLinkNotice
        who={notice.who}
        email={notice.email}
        expiresAt={notice.expiresAt}
        onClose={onClose}
      />
    );
  }
  return (
    <InvitationNotice
      who={notice.who}
      email={notice.email}
      invitation={notice.invitation}
      onClose={onClose}
    />
  );
}
