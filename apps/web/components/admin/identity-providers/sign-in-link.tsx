'use client';

import { useState } from 'react';
import { Button } from '@/components/ui';

/**
 * The company's sign-in link that names the provider (ADR-067), to hand out
 * to its collaborators: it signs in without typing an address and, when the
 * provider creates accounts, makes the account of whoever has none yet.
 */
export function SignInLink({ link, createsAccounts }: { link: string; createsAccounts: boolean }) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    try {
      await navigator.clipboard.writeText(link);
      setCopied(true);
    } catch {
      setCopied(false);
    }
  }

  return (
    <div className="flex flex-col gap-1">
      <p className="text-sm">Lien de connexion de l’entreprise :</p>
      <code className="w-fit max-w-full rounded bg-surface-2 px-2 py-1 text-xs break-all select-all">
        {link}
      </code>
      <p className="text-xs text-muted">
        {createsAccounts
          ? 'Diffusez-le à vos collaborateurs : leur compte est créé à leur première connexion, sans rôle ni unité, que vous leur donnez ensuite.'
          : 'Diffusez-le à vos collaborateurs : ils se connectent sans saisir leur adresse. Leur compte doit exister ici.'}
      </p>
      <Button variant="secondary" className="w-fit" onClick={copy}>
        {copied ? 'Lien copié' : 'Copier le lien'}
      </Button>
    </div>
  );
}
