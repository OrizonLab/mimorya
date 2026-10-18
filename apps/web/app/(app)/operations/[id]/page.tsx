'use client';

import { ArrowLeft } from 'lucide-react';
import Link from 'next/link';
import { useParams } from 'next/navigation';
import { RunApprovals, RunSummary, ToolInvocationsTable } from '@/components/operations';
import { Loaded, PageHeader } from '@/components/ui';
import { useApiData, type RunDetail } from '@/lib/api';

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
              title="Détail de l’opération"
              actions={
                detail.run.conversationId ? (
                  <Link
                    href={`/chat/${detail.run.conversationId}`}
                    className="text-sm text-accent underline"
                  >
                    Ouvrir la conversation
                  </Link>
                ) : undefined
              }
            />
            <RunSummary run={detail.run} />
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
