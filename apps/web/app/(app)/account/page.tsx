'use client';

import { PasswordForm } from '@/components/account';
import { PageHeader } from '@/components/ui';
import { useMe } from '@/lib/me-context';

export default function AccountPage() {
  const me = useMe();
  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-8">
      <PageHeader title="Mon compte" description={`${me.user.displayName} · ${me.user.email}`} />
      <PasswordForm />
    </div>
  );
}
