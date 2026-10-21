import { Card, LabelBadge } from '@/components/ui';
import type { Approval } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { APPROVAL_STATUS, RISK_LABELS } from '@/lib/labels';

/**
 * Who decided a request, when it was not simply whom the agent acts for
 * (ADR-059): their manager, or themselves withdrawing what a manager was to
 * decide.
 */
function decision(a: Approval): string | null {
  if (!a.decidedBy) return null;
  if (a.decidedBy.id !== a.requestedFor.id) {
    return `Pour ${a.requestedFor.displayName}, décidée par ${a.decidedBy.displayName}`;
  }
  return a.approver?.id === a.requestedFor.id ? null : `Retirée par ${a.decidedBy.displayName}`;
}

/** Decided (or expired) approval requests, most recent first. */
export function ApprovalHistory({ approvals }: { approvals: Approval[] }) {
  return (
    <Card className="mt-8 overflow-hidden">
      <h2 className="border-b border-line px-4 py-3 text-sm font-semibold">Historique</h2>
      <ul className="divide-y divide-line">
        {approvals.map((a) => (
          <li
            key={a.id}
            className="flex flex-col gap-1 px-4 py-3 text-sm sm:flex-row sm:items-center sm:gap-4"
          >
            <span className="min-w-0 flex-1">
              {a.actionSummary}
              {decision(a) && <span className="block text-xs text-muted">{decision(a)}</span>}
            </span>
            <LabelBadge labels={RISK_LABELS} value={a.risk} />
            <LabelBadge labels={APPROVAL_STATUS} value={a.status} />
            <span className="text-xs text-muted">{formatDate(a.decidedAt ?? a.createdAt)}</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}
