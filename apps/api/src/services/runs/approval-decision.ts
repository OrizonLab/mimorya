/**
 * Recording a human decision on an approval request, and deciding whether the
 * run it belongs to can resume.
 */
import type { ApprovalDecision } from '@mimorya/agent';
import { DomainError, NotFoundError } from '@mimorya/core';
import {
  appendAudit,
  decideApproval,
  expireApproval,
  getApproval,
  listApprovalsForRun,
  lockRun,
  updateRun,
  type ApprovalRow,
  type RunRow,
  type Transaction,
} from '@mimorya/db';
import type { AuthIdentity } from '../access';

export interface DecisionInput {
  approvalId: string;
  approved: boolean;
  comment?: string | undefined;
}

export interface RecordedDecision {
  /** The request had expired: it is recorded as such and counts as a refusal. */
  expired: boolean;
  decided: ApprovalRow | null;
  /** Set when every request of the run is settled and the run must resume. */
  resume: { run: RunRow; approvals: ApprovalRow[] } | null;
}

export async function recordApprovalDecision(
  tx: Transaction,
  identity: AuthIdentity,
  input: DecisionInput,
  now: Date,
): Promise<RecordedDecision> {
  const approval = await getApproval(tx, input.approvalId);
  // MVP rule: a user validates the actions requested on their own behalf.
  if (!approval || approval.requestedForUserId !== identity.userId) {
    throw new NotFoundError('Demande de validation introuvable');
  }
  // Concurrent decisions on the same run are serialised: exactly one of them
  // observes the last pending approval and resumes the run.
  const run = await lockRun(tx, approval.runId);
  const expired =
    approval.status === 'pending' && !!approval.expiresAt && approval.expiresAt <= now;

  let decided: ApprovalRow | null = null;
  if (expired) {
    await expireApproval(tx, approval.id);
  } else {
    decided = await decideApproval(tx, {
      id: approval.id,
      approved: input.approved,
      decidedByUserId: identity.userId,
      comment: input.comment ?? null,
    });
    if (!decided) throw new DomainError('conflict', 'Cette demande a déjà été traitée');
  }

  await appendAudit(tx, {
    tenantId: identity.tenantId,
    actorType: 'user',
    actorUserId: identity.userId,
    runId: approval.runId,
    action: expired
      ? 'approval.expired'
      : input.approved
        ? 'approval.approved'
        : 'approval.rejected',
    outcome: expired ? 'failure' : 'success',
    resourceType: 'approval',
    resourceId: approval.id,
    metadata: { toolName: approval.toolName, risk: approval.risk },
  });

  const approvals = await listApprovalsForRun(tx, approval.runId);
  const canResume =
    !!run &&
    run.status === 'awaiting_approval' &&
    !!run.state &&
    approvals.every((a) => a.status !== 'pending');
  if (!canResume) return { expired, decided, resume: null };

  await updateRun(tx, run.id, { status: 'running' });
  return { expired, decided, resume: { run, approvals } };
}

/** Decisions handed to the agent on resume; expired requests count as refusals. */
export function resumeDecisions(approvals: ApprovalRow[]): Record<string, ApprovalDecision> {
  const decisions: Record<string, ApprovalDecision> = {};
  for (const a of approvals) {
    if (a.status === 'expired') {
      decisions[a.id] = { approved: false, comment: 'demande de validation expirée' };
    } else if (a.status === 'approved' || a.status === 'rejected') {
      decisions[a.id] = {
        approved: a.status === 'approved',
        ...(a.decisionComment ? { comment: a.decisionComment } : {}),
      };
    }
  }
  return decisions;
}
