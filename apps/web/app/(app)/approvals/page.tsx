'use client';

import Link from 'next/link';
import { ApprovalCard, ApprovalHistory } from '@/components/approvals';
import { EmptyState, Loaded, PageHeader } from '@/components/ui';
import { useApiData, type ApprovalList } from '@/lib/api';
import { formatDate } from '@/lib/format';

export default function ApprovalsPage() {
  const state = useApiData<ApprovalList>('/approvals?status=all');

  return (
    <div className="mx-auto w-full max-w-4xl px-4 py-8">
      <PageHeader
        title="Validations"
        description="Les actions que l’agent souhaite réaliser pour vous et qui nécessitent votre accord. Les paramètres validés sont exécutés tels quels."
      />
      <Loaded state={state}>
        {({ approvals }) => {
          const pending = approvals.filter((a) => a.status === 'pending');
          const history = approvals.filter((a) => a.status !== 'pending');
          return (
            <>
              {pending.length === 0 && (
                <EmptyState title="Aucune action en attente de validation" />
              )}
              <div className="flex flex-col gap-3">
                {pending.map((a) => (
                  <div key={a.id}>
                    <ApprovalCard approval={a} onDecided={state.reload} />
                    <p className="mt-1 px-1 text-xs text-muted">
                      Demandée le {formatDate(a.createdAt)} · {a.reason} ·{' '}
                      <Link className="underline" href={`/operations/${a.runId}`}>
                        voir l’exécution
                      </Link>
                    </p>
                  </div>
                ))}
              </div>
              {history.length > 0 && <ApprovalHistory approvals={history} />}
            </>
          );
        }}
      </Loaded>
    </div>
  );
}
