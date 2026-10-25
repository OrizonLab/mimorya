import Link from 'next/link';
import { Badge, Card, LabelBadge } from '@/components/ui';
import type { RunListItem } from '@/lib/api';
import { formatCost } from '@/lib/format';
import { AGENT_PROFILES, RUN_STATUS } from '@/lib/labels';

/**
 * The sub-agents a run handed tasks to (ADR-098), in order: each with its
 * profile, its task and what it came to, linking to its own trace.
 */
export function SubRuns({ runs }: { runs: RunListItem[] }) {
  return (
    <section aria-labelledby="sub-runs" className="mb-6">
      <h2 id="sub-runs" className="mb-2 text-sm font-semibold">
        Sous-agents
      </h2>
      <Card className="overflow-hidden">
        <ul className="divide-y divide-line">
          {runs.map((run) => (
            <li key={run.id}>
              <Link
                href={`/operations/${run.id}`}
                className="flex flex-col gap-2 px-4 py-3 hover:bg-surface-2 sm:flex-row sm:items-center sm:gap-4"
              >
                <Badge tone="accent">{AGENT_PROFILES[run.agentProfile]}</Badge>
                <span className="line-clamp-2 min-w-0 flex-1 text-sm">{run.objective}</span>
                <span className="flex flex-wrap items-center gap-2 text-xs text-muted">
                  <LabelBadge labels={RUN_STATUS} value={run.status} />
                  <span>{run.usage.toolCalls} outil(s)</span>
                  <span>{formatCost(run.usage.costMicros)}</span>
                </span>
              </Link>
            </li>
          ))}
        </ul>
      </Card>
    </section>
  );
}
