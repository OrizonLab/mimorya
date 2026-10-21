'use client';

import { ShieldAlert } from 'lucide-react';
import { useState } from 'react';
import { Badge, Button, cx, DISABLED_LOOK, ErrorNote } from '@/components/ui';
import { api, ApiError, type ApprovalPerson, type ApprovalVerdict, type ToolRisk } from '@/lib/api';
import { RISK_LABELS } from '@/lib/labels';
import { useMe } from '@/lib/me-context';

export interface ApprovalLike {
  id: string;
  toolName: string;
  actionSummary: string;
  risk: ToolRisk;
  payload?: unknown;
  requestedFor: ApprovalPerson;
  approver: ApprovalPerson | null;
  canDecide: boolean;
}

/** Why the reader may not decide a pending request (ADR-059). */
function waiting(approval: ApprovalLike, me: string): string {
  if (!approval.approver) {
    return 'Plus personne ne peut valider cette demande : retirez-la pour que l’agent poursuive sans exécuter l’action.';
  }
  if (approval.approver.id !== me) {
    return `En attente de la validation de ${approval.approver.displayName}.`;
  }
  // Theirs to decide, but no longer allowed to.
  return approval.requestedFor.id === me
    ? 'Vous n’avez plus accès à l’agent : cette demande ne peut pas être décidée.'
    : 'Vous n’avez plus le droit de valider les actions de cette personne.';
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
  const me = useMe().user.id;
  // Decided by whom the agent acts for, or by their manager (ADR-059), and
  // always withdrawn by whom the agent acts for: the action is then not run.
  const canDecide = approval.canDecide;
  const withdrawable =
    !canDecide && approval.requestedFor.id === me && approval.approver?.id !== me;
  const risk = RISK_LABELS[approval.risk] ?? { label: approval.risk, tone: 'neutral' as const };

  async function decide(approved: boolean) {
    setBusy(approved ? 'approve' : 'reject');
    setError(null);
    try {
      await api.post(`/approvals/${approval.id}/decision`, {
        approved,
        comment: comment.trim() || undefined,
      } satisfies ApprovalVerdict);
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
          {approval.requestedFor.id !== me && (
            <p className="mt-1 text-xs text-muted">Pour {approval.requestedFor.displayName}</p>
          )}
          <p className="mt-1 text-sm">{approval.actionSummary}</p>
          {approval.payload !== undefined && (
            <details className="mt-2 text-xs text-muted">
              <summary className="cursor-pointer">Paramètres exacts qui seront exécutés</summary>
              <pre className="mt-1 max-h-48 overflow-auto rounded-md bg-surface p-2 text-[0.7rem]">
                {JSON.stringify(approval.payload, null, 2)}
              </pre>
            </details>
          )}
          {!canDecide && <p className="mt-3 text-sm text-muted">{waiting(approval, me)}</p>}
          {withdrawable && (
            <>
              {error && (
                <div className="mt-2">
                  <ErrorNote message={error} />
                </div>
              )}
              <div className="mt-3">
                <Button
                  variant="danger"
                  onClick={() => decide(false)}
                  loading={busy === 'reject'}
                  disabled={busy !== null}
                >
                  Retirer la demande
                </Button>
              </div>
            </>
          )}
          {canDecide && (
            <>
              <input
                value={comment}
                onChange={(e) => setComment(e.target.value)}
                placeholder="Commentaire (facultatif)"
                className={cx(
                  'mt-3 w-full rounded-lg border border-line bg-surface px-3 py-1.5 text-sm outline-none focus:border-accent',
                  DISABLED_LOOK,
                )}
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
