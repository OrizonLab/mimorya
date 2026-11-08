'use client';

import { SecretNotice } from '@/components/ui';
import type { BearerTokenCreated } from '@/lib/api';

/**
 * An export token just made: shown this once (the API keeps only its
 * digest, ADR-181), with how the SIEM reads the log at `exportUrl`, page
 * after page (`SecretNotice`).
 */
export function ExportTokenNotice({
  made,
  exportUrl,
  onClose,
}: {
  made: BearerTokenCreated;
  exportUrl: string;
  onClose: () => void;
}) {
  const example = `# La première page, depuis le début du journal :
curl -H "Authorization: Bearer ${made.secret}" \\
  "${exportUrl}?limit=500"
# Les suivantes, après la dernière place lue et son sceau :
curl -H "Authorization: Bearer ${made.secret}" \\
  "${exportUrl}?after=<place>&seal=<sceau>&limit=500"`;
  return (
    <SecretNotice
      level="h3"
      heading={`Jeton « ${made.token.name} »`}
      secret={made.secret}
      label="Jeton, à envoyer dans l’en-tête « Authorization: Bearer »"
      warning="Notez-le maintenant : il ne sera plus affiché. Qui le détient lit tout le journal d’audit de l’entreprise ; retirez-le s’il a pu être divulgué."
      example={{ summary: 'Exemple de lecture', text: example }}
      onClose={onClose}
    />
  );
}
