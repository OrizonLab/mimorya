import Link from 'next/link';
import { Card, LabelBadge } from '@/components/ui';
import type { RunSummary } from '@/lib/api';
import { formatCost, formatDate } from '@/lib/format';
import { RUN_STATUS } from '@/lib/labels';

export function RunList({ runs }: { runs: RunSummary[] }) {
  return (
    <Card className="overflow-hidden">
      <ul className="divide-y divide-line">
        {runs.map((run) => (
          <li key={run.id}>
            <Link
              href={`/operations/${run.id}`}
              className="flex flex-col gap-2 px-4 py-3 hover:bg-surface-2 sm:flex-row sm:items-center sm:gap-4"
            >
              <span className="line-clamp-2 min-w-0 flex-1 text-sm">{run.objective}</span>
              <span className="flex flex-wrap items-center gap-2 text-xs text-muted">
                <LabelBadge labels={RUN_STATUS} value={run.status} />
                <span>{run.usage.toolCalls} outil(s)</span>
                <span>{formatCost(run.usage.costMicros)}</span>
                <span>{formatDate(run.createdAt)}</span>
              </span>
            </Link>
          </li>
        ))}
      </ul>
    </Card>
  );
}
