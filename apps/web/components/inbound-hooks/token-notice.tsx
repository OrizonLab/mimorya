'use client';

import { SecretNotice } from '@/components/ui';
import type { InboundHookToken } from '@/lib/api';

/**
 * A hook's token, just made or renewed: shown this once (the API keeps only
 * its digest, ADR-109), with the address another system posts to and a
 * command that does it (`SecretNotice`).
 */
export function TokenNotice({
  title,
  issued,
  onClose,
}: {
  title: string;
  issued: InboundHookToken;
  onClose: () => void;
}) {
  const example = `curl -X POST ${issued.url} \\
  -H "Authorization: Bearer ${issued.token}" \\
  -H "Content-Type: application/json" \\
  --data '{"alerte": "Pompe P2 à l’arrêt", "site": "Lyon Nord"}'`;
  return (
    <SecretNotice
      className="mb-6"
      heading={`Jeton de « ${title} »`}
      secret={issued.token}
      label="Jeton, à envoyer dans l’en-tête « Authorization: Bearer »"
      warning="Notez-le maintenant : il ne sera plus affiché. Qui le détient peut confier des demandes à l’agent en votre nom ; renouvelez-le s’il a pu être divulgué."
      example={{ summary: 'Exemple d’envoi', text: example }}
      onClose={onClose}
    >
      <p className="text-sm">Adresse à appeler (méthode POST) :</p>
      <code className="w-fit max-w-full rounded bg-surface-2 px-2 py-1 text-xs break-all select-all">
        {issued.url}
      </code>
    </SecretNotice>
  );
}
