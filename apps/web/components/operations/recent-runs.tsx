import { Card } from '@/components/ui';
import type { RunListItem } from '@/lib/api';
import { RunRow } from './run-row';

/** The user's latest runs, each linking to its trace. */
export function RecentRuns({ runs }: { runs: RunListItem[] }) {
  return (
    <Card className="overflow-hidden">
      <ul className="divide-y divide-line">
        {runs.map((run) => (
          <li key={run.id}>
            <RunRow run={run} dated />
          </li>
        ))}
      </ul>
    </Card>
  );
}
