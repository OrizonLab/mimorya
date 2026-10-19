'use client';

import { ShieldAlert } from 'lucide-react';
import { useState } from 'react';
import { Badge, Button, ErrorNote } from '@/components/ui';
import { api, ApiError } from '@/lib/api';
import { RISK_LABELS } from '@/lib/labels';
import { useCan } from '@/lib/me-context';

export interface ApprovalLike {
  id: string;
  toolName: string;
  actionSummary: string;
  risk: string;
  payload?: unknown;
}

export function ApprovalCard({
  approval,
  onDecided,
}: {
  approval: ApprovalLike;
  onDecided: (approved: boolean) => void;
}) {
  const [comment, setComment] = useState('');
  const [busy, setBusy] = useState<'approve' | 'reject' | null>(null);
  const [error, setError] = useState<string | null>(null);
  // Deciding resumes the agent: without it, the request is left to expire.
  const canDecide = useCan('agent:use');
  const risk = RISK_LABELS[approval.risk] ?? { label: approval.risk, tone: 'neutral' as const };

  async function decide(approved: boolean) {
    setBusy(approved ? 'approve' : 'reject');
    setError(null);
    try {
      await api.post(`/approvals/${approval.id}/decision`, {
        approved,
        ...(comment.trim() ? { comment: comment.trim() } : {}),
      });
      onDecided(approved);
    } catch (e) {
      setError(e instanceof ApiError ? e.message : 'Décision impossible');
    } finally {
      setBusy(null);
    }
  }

  return (
    <div className="rounded-xl border border-warn/40 bg-warn-soft/60 p-4">
      <div className="flex items-start gap-3">
        <ShieldAlert className="mt-0.5 size-5 shrink-0 text-warn" aria-hidden />
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-sm font-semibold">Validation requise</p>
            <Badge tone={risk.tone}>{risk.label}</Badge>
          </div>
          <p className="mt-1 text-sm">{approval.actionSummary}</p>
          {approval.payload !== undefined && (
            <details className="mt-2 text-xs text-muted">
              <summary className="cursor-pointer">Paramètres exacts qui seront exécutés</summary>
              <pre className="mt-1 max-h-48 overflow-auto rounded-md bg-surface p-2 text-[0.7rem]">
                {JSON.stringify(approval.payload, null, 2)}
              </pre>
            </details>
          )}
          {!canDecide && (
            <p className="mt-3 text-sm text-muted">
              Vous n’avez plus accès à l’agent : cette demande ne peut pas être décidée et expirera
              d’elle-même.
            </p>
          )}
          {canDecide && (
            <>
              <input
                value={comment}
                onChange={(e) => setComment(e.target.value)}
                placeholder="Commentaire (facultatif)"
                className="mt-3 w-full rounded-lg border border-line bg-surface px-3 py-1.5 text-sm outline-none focus:border-accent"
              />
              {error && (
                <div className="mt-2">
                  <ErrorNote message={error} />
                </div>
              )}
              <div className="mt-3 flex flex-wrap gap-2">
                <Button
                  onClick={() => decide(true)}
                  loading={busy === 'approve'}
                  disabled={busy !== null}
                >
                  Valider
                </Button>
                <Button
                  variant="danger"
                  onClick={() => decide(false)}
                  loading={busy === 'reject'}
                  disabled={busy !== null}
                >
                  Refuser
                </Button>
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
