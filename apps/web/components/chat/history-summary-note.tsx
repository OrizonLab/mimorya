import { ScrollText } from 'lucide-react';
import type { HistorySummary } from '@/lib/api';

/**
 * Where a conversation's earlier messages stop being reread word for word
 * (ADR-145, ADR-146): the assistant rereads their summary instead, which its
 * owner may read too. Shown after the last message the summary covers, or
 * first when that message is no longer among those shown; in between, it
 * says when some of the messages it covers are no longer shown above it.
 * A summary that lags says how many of the messages after it are neither
 * summarised nor reread yet, until the assistant's next answers catch up
 * (ADR-149).
 */
export function HistorySummaryNote({
  summary,
  shown,
}: {
  summary: HistorySummary;
  /** How many of the messages it covers are shown above it; `null` when none. */
  shown: number | null;
}) {
  const count = summary.foldedMessages;
  const which =
    shown === null
      ? count > 1
        ? `Les ${count} premiers messages de cette conversation sont résumés`
        : 'Le premier message de cette conversation est résumé'
      : shown >= count
        ? count > 1
          ? `Les ${count} messages ci-dessus sont résumés`
          : 'Le message ci-dessus est résumé'
        : `Les ${count} premiers messages de cette conversation, jusqu’au message ci-dessus, sont résumés`;
  const lagging = summary.notReplayed;
  const behind =
    lagging > 1
      ? ` Les ${lagging} messages suivants ne sont pas encore résumés, et l’assistant ne les relit pas : ils le seront après ses prochaines réponses.`
      : lagging === 1
        ? ' Le message suivant n’est pas encore résumé, et l’assistant ne le relit pas : il le sera après sa prochaine réponse.'
        : '';
  return (
    <div
      role="note"
      className="flex flex-col gap-2 rounded-lg border border-line bg-surface-2 px-3 py-2 text-xs text-muted"
    >
      <p className="flex items-start gap-2">
        <ScrollText className="mt-0.5 size-3.5 shrink-0" aria-hidden />
        <span>
          {which} pour l’assistant : il ne {count > 1 ? 'les' : 'le'} relit plus mot pour mot, mais
          à travers ce résumé, établi par la plateforme.{behind}
        </span>
      </p>
      <details>
        <summary className="cursor-pointer select-none">Voir le résumé</summary>
        <p className="mt-2 whitespace-pre-wrap text-sm text-fg">{summary.summary}</p>
      </details>
    </div>
  );
}
