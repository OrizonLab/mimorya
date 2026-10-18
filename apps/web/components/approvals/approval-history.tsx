import { Card, LabelBadge } from '@/components/ui';
import type { Approval } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { APPROVAL_STATUS, RISK_LABELS } from '@/lib/labels';

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
            <span className="min-w-0 flex-1">{a.actionSummary}</span>
            <LabelBadge labels={RISK_LABELS} value={a.risk} />
            <LabelBadge labels={APPROVAL_STATUS} value={a.status} />
            <span className="text-xs text-muted">{formatDate(a.decidedAt ?? a.createdAt)}</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}
