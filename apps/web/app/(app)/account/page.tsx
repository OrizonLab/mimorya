'use client';

import { LinkedAccounts, NotificationForm, PasswordForm } from '@/components/account';
import { Card, PageHeader } from '@/components/ui';
import { useMe } from '@/lib/me-context';

export default function AccountPage() {
  const me = useMe();
  return (
    <div className="mx-auto w-full max-w-6xl px-4 py-8">
      <PageHeader title="Mon compte" description={`${me.user.displayName} · ${me.user.email}`} />
      <div className="flex flex-col gap-6">
        {me.user.hasPassword ? (
          <PasswordForm />
        ) : (
          // Accepted through the company's identity provider (ADR-064).
          <Card className="max-w-md p-4 text-sm">
            Votre compte n’a pas de mot de passe : vous vous connectez par le fournisseur d’identité
            de votre entreprise. Pour vous connecter aussi par mot de passe, demandez à votre
            administrateur un mot de passe provisoire.
          </Card>
        )}
        <LinkedAccounts />
        <NotificationForm />
      </div>
    </div>
  );
}
