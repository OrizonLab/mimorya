'use client';

import { CompanySignIn, LinkPage, useLinkToken } from '@/components/account';
import type { SsoLink, SsoLinkLookup } from '@/lib/api';

const asProvider = (provider: string) => ({ provider }) satisfies SsoLinkLookup;

/**
 * Opened from the company's sign-in link (`/login/entreprise#<provider>`,
 * ADR-067), before any sign-in.
 */
export default function CompanySignInPage() {
  const state = useLinkToken<SsoLink>(
    '/auth/sso/link',
    'Lien de connexion impossible à vérifier',
    asProvider,
  );
  return (
    <LinkPage state={state}>
      {(provider, link) => <CompanySignIn provider={provider} link={link} />}
    </LinkPage>
  );
}
