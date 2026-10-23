import Link from 'next/link';
import { LabelBadge } from '@/components/ui';
import type { ScheduledTask } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { OCCURRENCE_OUTCOME } from '@/lib/labels';

/**
 * What became of a task's last occurrence, and why it did not run, as the
 * platform says it; the conversation of the last run one started, which may
 * await a validation (ADR-080). Its moment is read in the task's time zone.
 */
export function LastOccurrence({ task }: { task: ScheduledTask }) {
  const last = task.lastOccurrence;
  if (!last && !task.lastConversationId) return null;
  return (
    <div className="flex flex-col gap-1 text-xs text-muted">
      {last && (
        <p className="flex flex-wrap items-center gap-2">
          Dernière occurrence, le {formatDate(last.at, task.timeZone)} :
          <LabelBadge labels={OCCURRENCE_OUTCOME} value={last.outcome} />
        </p>
      )}
      {last?.problem && <p>{last.problem}</p>}
      {task.lastConversationId && (
        <Link className="w-fit underline" href={`/chat/${task.lastConversationId}`}>
          Ouvrir la conversation du dernier lancement
        </Link>
      )}
    </div>
  );
}
