'use client';

import Link from 'next/link';
import { AbsenceSection, ApprovalCard, ApprovalHistory } from '@/components/approvals';
import { EmptyState, Loaded, PageHeader } from '@/components/ui';
import { useApiData, type ApprovalDelegations, type ApprovalList } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { useMe } from '@/lib/me-context';

export default function ApprovalsPage() {
  const state = useApiData<ApprovalList>('/approvals?status=all');
  const absence = useApiData<ApprovalDelegations>('/approval-delegation');
  const me = useMe().user.id;

  return (
    <div className="mx-auto w-full max-w-4xl px-4 py-8">
      <PageHeader
        title="Validations"
        description="Les actions que l’agent souhaite réaliser pour vous, ou pour les personnes dont vous validez les actions, et qui attendent un accord. Les paramètres validés sont exécutés tels quels."
      />
      <Loaded state={absence}>
        {(data) => <AbsenceSection data={data} onChanged={absence.reload} />}
      </Loaded>
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
                      Demandée le {formatDate(a.createdAt)} · {a.reason}
                      {/* Past it, the request expires and the agent goes on without the action. */}
                      {a.expiresAt && <> · expire le {formatDate(a.expiresAt)}</>}
                      {/* The run is shown to whom the agent acts for only. */}
                      {a.requestedFor.id === me && (
                        <>
                          {' · '}
                          <Link className="underline" href={`/operations/${a.runId}`}>
                            voir l’exécution
                          </Link>
                        </>
                      )}
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
