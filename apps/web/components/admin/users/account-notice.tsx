'use client';

import { InvitationNotice } from './invitation-notice';
import { TemporaryPassword } from './temporary-password';
import type { AccountNotice } from './types';

/** The temporary password or the invitation an administrator just obtained. */
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
  return (
    <InvitationNotice
      who={notice.who}
      email={notice.email}
      invitation={notice.invitation}
      onClose={onClose}
    />
  );
}
