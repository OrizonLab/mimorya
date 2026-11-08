'use client';

import type { ScimTokens as ScimTokenList } from '@/lib/api';
import { BearerTokens, type BearerTokenWords } from '../bearer-tokens/bearer-tokens';
import { ScimTokenNotice } from './scim-token-notice';

const WORDS: BearerTokenWords = {
  title: 'Provisionnement SCIM',
  about:
    'Le fournisseur d’identité de l’entreprise (Microsoft Entra ID, Okta…) crée, change, désactive et retire ses comptes par SCIM 2.0, avec un jeton. Un compte qu’il crée n’a que le rôle de base, sans mot de passe : la personne se connecte par ce fournisseur. Il ne change jamais l’adresse d’un compte, et un compte qu’il retire est désactivé, jamais supprimé.',
  none: 'Aucun jeton : aucun fournisseur d’identité ne provisionne les comptes.',
  holder: 'le fournisseur d’identité qui le détiendra',
  removal: (name) =>
    `Retirer le jeton « ${name} » ? Le fournisseur d’identité qui le détient ne créera ni ne changera plus aucun compte.`,
};

/**
 * The tokens with which the company's identity provider provisions its
 * accounts through SCIM (ADR-183), for who manages the company's accounts
 * (`user:manage`, company-wide), as any token another system holds of the
 * company (`BearerTokens`): first the address the provider reaches SCIM at.
 */
export function ScimTokens() {
  return (
    <BearerTokens<ScimTokenList>
      path="/admin/scim/tokens"
      words={WORDS}
      details={(data) => (
        <p className="text-sm">
          Adresse SCIM à donner au fournisseur d’identité :{' '}
          <code className="rounded bg-surface-2 px-1 text-xs break-all select-all">
            {data.scimUrl}
          </code>
        </p>
      )}
      notice={(made, data, onClose) => (
        <ScimTokenNotice made={made} scimUrl={data.scimUrl} onClose={onClose} />
      )}
    />
  );
}
