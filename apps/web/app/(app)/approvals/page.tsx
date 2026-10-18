'use client';

import Link from 'next/link';
import { useCallback, useEffect, useState } from 'react';
import { ApprovalCard } from '@/components/chat/approval-card';
import { Badge, Card, EmptyState, PageHeader, RISK_LABELS, Spinner } from '@/components/ui';
import { api, formatDate, type Approval } from '@/lib/api';

const STATUS: Record<string, { label: string; tone: 'ok' | 'danger' | 'neutral' | 'warn' }> = {
  pending: { label: 'En attente', tone: 'warn' },
  approved: { label: 'Validée', tone: 'ok' },
  rejected: { label: 'Refusée', tone: 'danger' },
  expired: { label: 'Expirée', tone: 'neutral' },
  cancelled: { label: 'Annulée', tone: 'neutral' },
};

export default function ApprovalsPage() {
  const [items, setItems] = useState<Approval[] | null>(null);

  const load = useCallback(() => {
    api
      .get<{ approvals: Approval[] }>('/approvals?status=all')
      .then((r) => setItems(r.approvals))
      .catch(() => setItems([]));
  }, []);
  useEffect(load, [load]);

  const pending = items?.filter((a) => a.status === 'pending') ?? [];
  const history = items?.filter((a) => a.status !== 'pending') ?? [];

  return (
    <div className="mx-auto w-full max-w-4xl px-4 py-8">
      <PageHeader
        title="Validations"
        description="Les actions que l’agent souhaite réaliser pour vous et qui nécessitent votre accord. Les paramètres validés sont exécutés tels quels."
      />
      {items === null && <Spinner />}
      {items && pending.length === 0 && (
        <EmptyState title="Aucune action en attente de validation" />
      )}
      <div className="flex flex-col gap-3">
        {pending.map((a) => (
          <div key={a.id}>
            <ApprovalCard approval={a} onDecided={load} />
            <p className="mt-1 px-1 text-xs text-muted">
              Demandée le {formatDate(a.createdAt)} · {a.reason} ·{' '}
              <Link className="underline" href={`/operations/${a.runId}`}>
                voir l’exécution
              </Link>
            </p>
          </div>
        ))}
      </div>
      {history.length > 0 && (
        <Card className="mt-8 overflow-hidden">
          <h2 className="border-b border-line px-4 py-3 text-sm font-semibold">Historique</h2>
          <ul className="divide-y divide-line">
            {history.map((a) => (
              <li
                key={a.id}
                className="flex flex-col gap-1 px-4 py-3 text-sm sm:flex-row sm:items-center sm:gap-4"
              >
                <span className="min-w-0 flex-1">{a.actionSummary}</span>
                <Badge tone={RISK_LABELS[a.risk]?.tone ?? 'neutral'}>
                  {RISK_LABELS[a.risk]?.label ?? a.risk}
                </Badge>
                <Badge tone={STATUS[a.status]?.tone ?? 'neutral'}>
                  {STATUS[a.status]?.label ?? a.status}
                </Badge>
                <span className="text-xs text-muted">{formatDate(a.decidedAt ?? a.createdAt)}</span>
              </li>
            ))}
          </ul>
        </Card>
      )}
    </div>
  );
}
