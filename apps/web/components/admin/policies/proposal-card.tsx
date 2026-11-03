'use client';

import { Badge, Button, Card, ErrorNote, LabelBadge } from '@/components/ui';
import { api, useAction, type Policies, type PolicyProposalView } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { PROPOSAL_STATUS } from '@/lib/labels';
import { describeCondition } from './describe';

type Decision = 'approval' | 'rejection' | 'withdrawal';

/**
 * One proposed allow policy (ADR-168): what it will grant, and when, who
 * proposed it and where it stands. Another administrator than its author
 * approves or rejects it; its author withdraws it.
 */
export function ProposalCard({
  proposal,
  data,
  onDecided,
}: {
  proposal: PolicyProposalView;
  data: Policies;
  onDecided: () => void;
}) {
  const action = useAction();
  const permissionLabels = new Map(data.permissions.map((p) => [p.permission, p.label]));
  const attributeLabels = new Map(data.attributes.map((a) => [a.path, a.label]));
  const changed = proposal.policyId
    ? data.policies.find((policy) => policy.id === proposal.policyId)
    : undefined;
  const waiting = proposal.status === 'pending';

  async function decide(decision: Decision, question: string) {
    if (!window.confirm(question)) return;
    const done = await action.run(() =>
      api.post(`/admin/policy-proposals/${proposal.id}/${decision}`),
    );
    if (done) onDecided();
  }

  return (
    <Card className="p-4">
      <div className="flex flex-wrap items-center gap-2">
        <p className="font-medium">{proposal.description}</p>
        <LabelBadge labels={PROPOSAL_STATUS} value={proposal.status} />
        {proposal.outdated && <Badge tone="warn">Périmée</Badge>}
        {!proposal.enabled && <Badge tone="neutral">Inactive une fois approuvée</Badge>}
        {waiting && (
          <span className="ml-auto flex gap-1">
            {proposal.mine ? (
              <Button
                variant="ghost"
                className="px-2 py-1"
                loading={action.pending}
                onClick={() =>
                  decide('withdrawal', `Retirer la proposition « ${proposal.description} » ?`)
                }
              >
                Retirer
              </Button>
            ) : (
              <>
                <Button
                  variant="ghost"
                  className="px-2 py-1"
                  loading={action.pending}
                  disabled={proposal.outdated}
                  onClick={() =>
                    decide(
                      'approval',
                      `Approuver « ${proposal.description} » ? L’autorisation s’applique aussitôt.`,
                    )
                  }
                >
                  Approuver
                </Button>
                <Button
                  variant="ghost"
                  className="px-2 py-1"
                  loading={action.pending}
                  onClick={() => decide('rejection', `Refuser « ${proposal.description} » ?`)}
                >
                  Refuser
                </Button>
              </>
            )}
          </span>
        )}
      </div>
      <p className="mt-1 text-xs text-muted">
        {proposal.policyId
          ? `Modifie l’autorisation « ${changed?.description ?? 'supprimée'} »`
          : 'Nouvelle autorisation'}{' '}
        · proposée par {proposal.proposedBy.displayName} le {formatDate(proposal.proposedAt)}
        {waiting && ` · expire le ${formatDate(proposal.expiresAt)}`}
        {proposal.decidedBy &&
          ` · décidée par ${proposal.decidedBy.displayName} le ${formatDate(proposal.decidedAt)}`}
      </p>
      <p className="mt-2 text-sm">
        <span className="text-muted">Quand : </span>
        {describeCondition(proposal.condition, attributeLabels)}
      </p>
      <ul className="mt-2 flex flex-wrap gap-1">
        {proposal.permissions.map((p) => (
          <li key={p}>
            <Badge>{permissionLabels.get(p) ?? p}</Badge>
          </li>
        ))}
      </ul>
      {waiting && proposal.outdated && (
        <p className="mt-2 text-xs text-muted">
          La politique a changé depuis cette proposition : elle ne peut plus être approuvée. Il faut
          en faire une nouvelle.
        </p>
      )}
      {waiting && proposal.mine && !proposal.outdated && (
        <p className="mt-2 text-xs text-muted">
          Un autre administrateur des politiques doit l’approuver.
        </p>
      )}
      {action.error && <ErrorNote message={action.error} />}
    </Card>
  );
}
