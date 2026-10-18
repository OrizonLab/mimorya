'use client';

import Link from 'next/link';
import { useEffect, useState } from 'react';
import { Badge, Card, EmptyState, PageHeader, RUN_STATUS, Spinner } from '@/components/ui';
import { api, formatCost, formatDate, type RunStatus, type RunUsage } from '@/lib/api';

interface RunSummary {
  id: string;
  conversationId: string | null;
  objective: string;
  status: RunStatus;
  usage: RunUsage;
  error: string | null;
  createdAt: string;
  finishedAt: string | null;
}

export default function OperationsPage() {
  const [runs, setRuns] = useState<RunSummary[] | null>(null);

  useEffect(() => {
    api
      .get<{ runs: RunSummary[] }>('/runs')
      .then((r) => setRuns(r.runs))
      .catch(() => setRuns([]));
  }, []);

  return (
    <div className="mx-auto w-full max-w-5xl px-4 py-8">
      <PageHeader
        title="Opérations"
        description="Les travaux réalisés par l’agent pour vous : étapes, outils utilisés, validations, coût. Tout est tracé."
      />
      {runs === null && <Spinner />}
      {runs?.length === 0 && <EmptyState title="Aucune opération pour le moment" />}
      {runs && runs.length > 0 && (
        <Card className="overflow-hidden">
          <ul className="divide-y divide-line">
            {runs.map((run) => {
              const status = RUN_STATUS[run.status] ?? {
                label: run.status,
                tone: 'neutral' as const,
              };
              return (
                <li key={run.id}>
                  <Link
                    href={`/operations/${run.id}`}
                    className="flex flex-col gap-2 px-4 py-3 hover:bg-surface-2 sm:flex-row sm:items-center sm:gap-4"
                  >
                    <span className="line-clamp-2 min-w-0 flex-1 text-sm">{run.objective}</span>
                    <span className="flex flex-wrap items-center gap-2 text-xs text-muted">
                      <Badge tone={status.tone}>{status.label}</Badge>
                      <span>{run.usage.toolCalls} outil(s)</span>
                      <span>{formatCost(run.usage.costMicros)}</span>
                      <span>{formatDate(run.createdAt)}</span>
                    </span>
                  </Link>
                </li>
              );
            })}
          </ul>
        </Card>
      )}
    </div>
  );
}
