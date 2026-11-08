'use client';

import { SecretNotice } from '@/components/ui';
import type { BearerTokenCreated } from '@/lib/api';

/**
 * A SCIM token just made: shown this once (the API keeps only its digest,
 * ADR-183), with where the identity provider takes it and the address it
 * reaches SCIM at, `scimUrl` (`SecretNotice`).
 */
export function ScimTokenNotice({
  made,
  scimUrl,
  onClose,
}: {
  made: BearerTokenCreated;
  scimUrl: string;
  onClose: () => void;
}) {
  const example = `# Les comptes de l’entreprise, tels que le fournisseur les lit :
curl -H "Authorization: Bearer ${made.secret}" \\
  "${scimUrl}/Users?count=10"`;
  return (
    <SecretNotice
      level="h3"
      heading={`Jeton « ${made.token.name} »`}
      secret={made.secret}
      label="Jeton secret, à donner au fournisseur d’identité"
      warning="Notez-le maintenant : il ne sera plus affiché. Qui le détient crée, change et désactive les comptes de l’entreprise ; retirez-le s’il a pu être divulgué."
      example={{ summary: 'Exemple de lecture', text: example }}
      onClose={onClose}
    >
      <p className="text-sm">
        Dans le fournisseur d’identité, donnez l’adresse{' '}
        <code className="rounded bg-surface-2 px-1 text-xs break-all">{scimUrl}</code> (« URL du
        locataire » dans Microsoft Entra ID, « Base URL » dans Okta) et ce jeton.
      </p>
    </SecretNotice>
  );
}
