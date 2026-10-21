'use client';

import Link from 'next/link';
import { Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type SsoLink, type SsoLinkLookup, type SsoStart } from '@/lib/api';

/**
 * The company's sign-in link (ADR-067): it names the company and its
 * identity provider, which signs the person in; when the company asked
 * for it, a first sign-in creates their account. Never from an address
 * alone: a domain is declared, not proven.
 */
export function CompanySignIn({ provider, link }: { provider: string; link: SsoLink }) {
  const action = useAction();
  const name = link.provider.name;

  async function start() {
    const started = await action.run(() =>
      api.post<SsoStart>('/auth/sso/link/start', { provider } satisfies SsoLinkLookup),
    );
    if (started) window.location.assign(started.redirectUrl);
  }

  return (
    <Card className="flex flex-col gap-4 p-6">
      <div className="text-sm">
        <p className="font-medium">{link.company}</p>
        <p className="text-muted">
          Connectez-vous avec {name}, le fournisseur d’identité de votre entreprise.{' '}
          {link.createsAccounts
            ? 'À votre première connexion, votre compte est créé avec l’adresse professionnelle que le fournisseur garantit ; votre entreprise vous donne ensuite vos accès.'
            : 'Votre compte doit avoir été créé par votre entreprise.'}
        </p>
      </div>
      <Button type="button" loading={action.pending} onClick={start}>
        Se connecter avec {name}
      </Button>
      {action.error && <ErrorNote message={action.error} />}
      <Link href="/login" className="text-center text-sm text-muted hover:underline">
        Se connecter autrement
      </Link>
    </Card>
  );
}
