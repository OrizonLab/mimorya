'use client';

import { ArrowLeft } from 'lucide-react';
import Link from 'next/link';
import { useParams } from 'next/navigation';
import { useEffect, useState, type ReactNode } from 'react';
import {
  Badge,
  Card,
  ErrorNote,
  PageHeader,
  RISK_LABELS,
  RUN_STATUS,
  Spinner,
} from '@/components/ui';
import {
  api,
  ApiError,
  formatCost,
  formatDate,
  type Approval,
  type RunStatus,
  type RunUsage,
} from '@/lib/api';

interface RunDetail {
  run: {
    id: string;
    conversationId: string | null;
    objective: string;
    status: RunStatus;
    autonomyLevel: string;
    usage: RunUsage;
    verification: { passed: boolean; issues: string[]; judged: boolean } | null;
    error: string | null;
    createdAt: string;
    finishedAt: string | null;
  };
  tools: Array<{
    id: string;
    toolName: string;
    decision: string;
    decisionReason: string | null;
    status: string;
    outputSummary: string | null;
    durationMs: number | null;
    createdAt: string;
  }>;
  approvals: Approval[];
}

const AUTONOMY: Record<string, string> = {
  read_only: 'Lecture seule',
  propose: 'Proposition',
  execute_with_approval: 'Exécution après validation',
  autonomous_limited: 'Autonomie limitée',
};

const TOOL_STATUS: Record<string, { label: string; tone: 'ok' | 'danger' | 'warn' | 'neutral' }> = {
  succeeded: { label: 'Réussi', tone: 'ok' },
  failed: { label: 'Échec', tone: 'danger' },
  denied: { label: 'Refusé', tone: 'danger' },
  awaiting_approval: { label: 'Validation demandée', tone: 'warn' },
  rejected: { label: 'Rejeté', tone: 'neutral' },
};

export default function OperationDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [detail, setDetail] = useState<RunDetail | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    api
      .get<RunDetail>(`/runs/${id}`)
      .then(setDetail)
      .catch((e: unknown) => setError(e instanceof ApiError ? e.message : 'Chargement impossible'));
  }, [id]);

  if (error) {
    return (
      <div className="mx-auto w-full max-w-5xl px-4 py-8">
        <ErrorNote message={error} />
      </div>
    );
  }
  if (!detail) return <Spinner />;
  const { run } = detail;
  const status = RUN_STATUS[run.status] ?? { label: run.status, tone: 'neutral' as const };

  return (
    <div className="mx-auto w-full max-w-5xl px-4 py-8">
      <Link
        href="/operations"
        className="mb-4 inline-flex items-center gap-1 text-sm text-muted hover:text-fg"
      >
        <ArrowLeft className="size-4" aria-hidden /> Opérations
      </Link>
      <PageHeader
        title="Détail de l’opération"
        actions={
          run.conversationId ? (
            <Link href={`/chat/${run.conversationId}`} className="text-sm text-accent underline">
              Ouvrir la conversation
            </Link>
          ) : undefined
        }
      />
      <Card className="mb-6 p-4">
        <p className="text-sm">{run.objective}</p>
        <dl className="mt-4 grid grid-cols-2 gap-3 text-sm sm:grid-cols-4">
          <Stat label="Statut" value={<Badge tone={status.tone}>{status.label}</Badge>} />
          <Stat label="Autonomie" value={AUTONOMY[run.autonomyLevel] ?? run.autonomyLevel} />
          <Stat label="Étapes / outils" value={`${run.usage.steps} / ${run.usage.toolCalls}`} />
          <Stat label="Coût estimé" value={formatCost(run.usage.costMicros)} />
          <Stat
            label="Tokens (entrée / sortie)"
            value={`${run.usage.inputTokens.toLocaleString('fr-FR')} / ${run.usage.outputTokens.toLocaleString('fr-FR')}`}
          />
          <Stat label="Début" value={formatDate(run.createdAt)} />
          <Stat label="Fin" value={formatDate(run.finishedAt)} />
          <Stat
            label="Vérification"
            value={
              run.verification ? (
                <Badge tone={run.verification.passed ? 'ok' : 'warn'}>
                  {run.verification.passed ? 'Réussie' : 'Points non vérifiés'}
                </Badge>
              ) : (
                '—'
              )
            }
          />
        </dl>
        {run.error && (
          <div className="mt-4">
            <ErrorNote message={run.error} />
          </div>
        )}
      </Card>

      <h2 className="mb-2 text-sm font-semibold">Outils utilisés</h2>
      <Card className="mb-6 overflow-x-auto">
        <table className="w-full text-sm">
          <thead className="bg-surface-2 text-left text-xs text-muted">
            <tr>
              <th className="px-4 py-2 font-medium">Heure</th>
              <th className="px-4 py-2 font-medium">Outil</th>
              <th className="px-4 py-2 font-medium">Résultat</th>
              <th className="px-4 py-2 font-medium">Détail</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-line">
            {detail.tools.map((t) => {
              const s = TOOL_STATUS[t.status] ?? { label: t.status, tone: 'neutral' as const };
              return (
                <tr key={t.id}>
                  <td className="px-4 py-2 whitespace-nowrap text-muted">
                    {new Date(t.createdAt).toLocaleTimeString('fr-FR')}
                  </td>
                  <td className="px-4 py-2 font-mono text-xs">{t.toolName}</td>
                  <td className="px-4 py-2">
                    <Badge tone={s.tone}>{s.label}</Badge>
                  </td>
                  <td className="px-4 py-2 text-muted">
                    {t.outputSummary ?? t.decisionReason ?? '—'}
                    {t.durationMs !== null && <span className="text-xs"> · {t.durationMs} ms</span>}
                  </td>
                </tr>
              );
            })}
            {detail.tools.length === 0 && (
              <tr>
                <td colSpan={4} className="px-4 py-4 text-center text-muted">
                  Aucun outil utilisé.
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </Card>

      {detail.approvals.length > 0 && (
        <>
          <h2 className="mb-2 text-sm font-semibold">Validations</h2>
          <Card className="divide-y divide-line">
            {detail.approvals.map((a) => (
              <div
                key={a.id}
                className="flex flex-col gap-1 px-4 py-3 text-sm sm:flex-row sm:items-center sm:gap-3"
              >
                <span className="flex-1">{a.actionSummary}</span>
                <Badge tone={RISK_LABELS[a.risk]?.tone ?? 'neutral'}>
                  {RISK_LABELS[a.risk]?.label ?? a.risk}
                </Badge>
                <Badge
                  tone={
                    a.status === 'approved' ? 'ok' : a.status === 'pending' ? 'warn' : 'neutral'
                  }
                >
                  {a.status === 'approved'
                    ? 'Validée'
                    : a.status === 'pending'
                      ? 'En attente'
                      : 'Refusée'}
                </Badge>
              </div>
            ))}
          </Card>
        </>
      )}
    </div>
  );
}

function Stat({ label, value }: { label: string; value: ReactNode }) {
  return (
    <div>
      <dt className="text-xs text-muted">{label}</dt>
      <dd className="mt-0.5">{value}</dd>
    </div>
  );
}
