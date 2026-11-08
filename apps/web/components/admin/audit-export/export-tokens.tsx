'use client';

import type { AuditExportTokens } from '@/lib/api';
import { BearerTokens, type BearerTokenWords } from '../bearer-tokens/bearer-tokens';
import { ExportTokenNotice } from './export-token-notice';

const WORDS: BearerTokenWords = {
  title: 'Export vers un SIEM',
  about:
    'Votre outil de sécurité (SIEM) lit le journal scellé avec un jeton, page après page, dans l’ordre de la chaîne. Chaque entrée porte sa place et son sceau : il reprend après la dernière place lue, en donnant son sceau, et garde les sceaux comme ancres à vérifier.',
  none: 'Aucun jeton : le journal n’est lu par aucun SIEM.',
  holder: 'le SIEM qui le détiendra',
  removal: (name) => `Retirer le jeton « ${name} » ? Le SIEM qui le détient ne lira plus rien.`,
};

/**
 * The tokens with which the company's SIEM reads its audit log (ADR-181,
 * ADR-182), for who may export it (`audit:export`), as any token another
 * system holds of the company (`BearerTokens`): first whether this server
 * seals the log, and the address the SIEM reads. `onChanged` reloads the
 * audit log, when it is shown.
 */
export function ExportTokens({ onChanged }: { onChanged?: () => void }) {
  return (
    <BearerTokens<AuditExportTokens>
      path="/admin/audit/export-tokens"
      words={WORDS}
      onChanged={onChanged}
      details={(data) => (
        <>
          {!data.sealing && (
            <p role="note" className="text-sm text-warn">
              Ce serveur ne scelle pas le journal d’audit : aucune clé maître n’y est configurée,
              rien n’en est exporté. Adressez-vous à l’exploitant de la plateforme.
            </p>
          )}
          <p className="text-sm">
            Adresse à lire (méthode GET) :{' '}
            <code className="rounded bg-surface-2 px-1 text-xs break-all select-all">
              {data.exportUrl}
            </code>
          </p>
        </>
      )}
      notice={(made, data, onClose) => (
        <ExportTokenNotice made={made} exportUrl={data.exportUrl} onClose={onClose} />
      )}
    />
  );
}
