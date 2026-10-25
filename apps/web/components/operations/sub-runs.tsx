import { Card } from '@/components/ui';
import type { RunListItem } from '@/lib/api';
import { RunRow } from './run-row';

/**
 * The sub-agents a run handed tasks to (ADR-098), in order: each with its
 * profile, its task and what it came to, linking to its own trace. What
 * they spent is counted in their run's own cost already.
 */
export function SubRuns({ runs }: { runs: RunListItem[] }) {
  return (
    <section aria-labelledby="sub-runs" className="mb-6">
      <h2 id="sub-runs" className="mb-1 text-sm font-semibold">
        Sous-agents
      </h2>
      <p className="mb-2 text-xs text-muted">Leur coût est compris dans celui de la demande.</p>
      <Card className="overflow-hidden">
        <ul className="divide-y divide-line">
          {runs.map((run) => (
            <li key={run.id}>
              <RunRow run={run} profiled />
            </li>
          ))}
        </ul>
      </Card>
    </section>
  );
}
