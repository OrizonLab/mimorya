import { ScrollText } from 'lucide-react';
import type { HistorySummary } from '@/lib/api';

/**
 * Where a conversation's earlier messages stop being reread word for word
 * (ADR-145, ADR-146): the assistant rereads their summary instead, which its
 * owner may read too. Shown after the last message the summary covers, or
 * first when that message is no longer among those shown.
 */
export function HistorySummaryNote({
  summary,
  above,
}: {
  summary: HistorySummary;
  /** Whether the messages it covers are shown above it. */
  above: boolean;
}) {
  const count = summary.foldedMessages;
  const messages = count > 1 ? `${count} messages` : 'message';
  const which = above
    ? count > 1
      ? `Les ${messages} ci-dessus sont résumés`
      : 'Le message ci-dessus est résumé'
    : count > 1
      ? `Les ${messages} les plus anciens de cette conversation sont résumés`
      : 'Le message le plus ancien de cette conversation est résumé';
  return (
    <div
      role="note"
      className="flex flex-col gap-2 rounded-lg border border-line bg-surface-2 px-3 py-2 text-xs text-muted"
    >
      <p className="flex items-start gap-2">
        <ScrollText className="mt-0.5 size-3.5 shrink-0" aria-hidden />
        <span>
          {which} pour l’assistant : il ne {count > 1 ? 'les' : 'le'} relit plus mot pour mot, mais
          à travers ce résumé, établi par la plateforme.
        </span>
      </p>
      <details>
        <summary className="cursor-pointer select-none">Voir le résumé</summary>
        <p className="mt-2 whitespace-pre-wrap text-sm text-fg">{summary.summary}</p>
      </details>
    </div>
  );
}
