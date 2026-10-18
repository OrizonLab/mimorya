'use client';

import { KeyRound, LogOut } from 'lucide-react';
import { Button, PageHeader } from '@/components/ui';
import type { Me } from '@/lib/api';
import { PasswordForm } from './password-form';

/**
 * Shown instead of the application while the account holds a temporary
 * password: the API refuses everything else until it is replaced.
 */
export function FirstPasswordScreen({ me, onLogout }: { me: Me; onLogout: () => void }) {
  return (
    <main className="mx-auto flex w-full max-w-xl flex-col gap-4 px-4 py-10">
      <PageHeader title="Choisissez votre mot de passe" description={me.user.email} />
      <p className="flex items-start gap-2 rounded-lg border border-warn bg-surface p-3 text-sm">
        <KeyRound className="mt-0.5 size-4 shrink-0" aria-hidden />
        Votre mot de passe est provisoire : il vous a été transmis par un administrateur.
        Remplacez-le par un mot de passe personnel, que personne d’autre ne connaît, pour accéder à
        votre espace.
      </p>
      <PasswordForm onChanged={() => window.location.assign('/chat')} />
      <Button variant="ghost" className="w-fit" onClick={onLogout}>
        <LogOut className="size-4" aria-hidden /> Se déconnecter
      </Button>
    </main>
  );
}
