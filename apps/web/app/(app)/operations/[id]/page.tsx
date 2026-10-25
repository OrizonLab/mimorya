'use client';

import { ArrowLeft } from 'lucide-react';
import Link from 'next/link';
import { useParams } from 'next/navigation';
import {
  RunApprovals,
  RunOrigin,
  RunSummary,
  SubRuns,
  ToolInvocationsTable,
} from '@/components/operations';
import { Loaded, PageHeader } from '@/components/ui';
import { useApiData, type RunDetail } from '@/lib/api';
import { AGENT_PROFILES } from '@/lib/labels';

export default function OperationDetailPage() {
  const { id } = useParams<{ id: string }>();
  const state = useApiData<RunDetail>(`/runs/${id}`);

  return (
    <div className="mx-auto w-full max-w-5xl px-4 py-8">
      <Link
        href="/operations"
        className="mb-4 inline-flex items-center gap-1 text-sm text-muted hover:text-fg"
      >
        <ArrowLeft className="size-4" aria-hidden /> Opérations
      </Link>
      <Loaded state={state}>
        {(detail) => (
          <>
            <PageHeader
              title={
                detail.run.parentRunId
                  ? `Sous-agent « ${AGENT_PROFILES[detail.run.agentProfile]} »`
                  : 'Détail de l’opération'
              }
              actions={<RunOrigin run={detail.run} />}
            />
            <RunSummary run={detail.run} />
            {detail.subRuns.length > 0 && <SubRuns runs={detail.subRuns} />}
            <h2 className="mb-2 text-sm font-semibold">Outils utilisés</h2>
            <ToolInvocationsTable tools={detail.tools} />
            {detail.approvals.length > 0 && (
              <>
                <h2 className="mb-2 text-sm font-semibold">Validations</h2>
                <RunApprovals approvals={detail.approvals} />
              </>
            )}
          </>
        )}
      </Loaded>
    </div>
  );
}
