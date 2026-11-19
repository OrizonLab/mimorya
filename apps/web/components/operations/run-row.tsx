import Link from 'next/link';
import { Badge, LabelBadge } from '@/components/ui';
import type { RunListItem } from '@/lib/api';
import { formatCost, formatDate } from '@/lib/format';
import { AGENT_PROFILES, RUN_STATUS } from '@/lib/labels';

/**
 * One run of a list, linking to its trace: what it was asked, its status,
 * tools and cost; its sub-agent profile (ADR-099) and its date if asked.
 */
export function RunRow({
  run,
  profiled = false,
  dated = false,
}: {
  run: RunListItem;
  profiled?: boolean;
  dated?: boolean;
}) {
  return (
    <Link
      href={`/operations/${run.id}`}
      className="flex flex-col gap-2 px-4 py-3 hover:bg-surface-2 sm:flex-row sm:items-center sm:gap-4"
    >
      {profiled && <Badge tone="accent">{AGENT_PROFILES[run.agentProfile]}</Badge>}
      <span className="line-clamp-2 min-w-0 flex-1 text-sm">{run.objective}</span>
      <span className="flex flex-wrap items-center gap-2 text-xs text-muted">
        <LabelBadge labels={RUN_STATUS} value={run.status} />
        <span>{run.usage.toolCalls} outil(s)</span>
        <span>{formatCost(run.usage.costMicros)}</span>
        {dated && <span>{formatDate(run.createdAt)}</span>}
      </span>
    </Link>
  );
}
