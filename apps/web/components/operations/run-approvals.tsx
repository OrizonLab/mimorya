import { Card, LabelBadge } from '@/components/ui';
import type { Approval } from '@/lib/api';
import { APPROVAL_STATUS, RISK_LABELS } from '@/lib/labels';

/** Approval requests raised during a run and their outcome. */
export function RunApprovals({ approvals }: { approvals: Approval[] }) {
  return (
    <Card className="divide-y divide-line">
      {approvals.map((a) => (
        <div
          key={a.id}
          className="flex flex-col gap-1 px-4 py-3 text-sm sm:flex-row sm:items-center sm:gap-3"
        >
          <span className="flex-1">{a.actionSummary}</span>
          <LabelBadge labels={RISK_LABELS} value={a.risk} />
          <LabelBadge labels={APPROVAL_STATUS} value={a.status} />
        </div>
      ))}
    </Card>
  );
}
