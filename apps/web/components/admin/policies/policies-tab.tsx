'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Button, Card, EmptyState, Loaded } from '@/components/ui';
import { useApiData, type Policies, type PolicyProposals } from '@/lib/api';
import { GrantForm } from './grant-form';
import { PolicyCard } from './policy-card';
import { PolicyForm } from './policy-form';
import { ProposalList } from './proposal-list';

type Creating = 'deny' | 'allow' | null;

/**
 * Access policies on top of the roles: deny policies, written at once, and
 * allow policies, proposed by one administrator and approved by another
 * (ADR-022, ADR-168).
 */
export function PoliciesTab() {
  const state = useApiData<Policies>('/admin/policies');
  const proposals = useApiData<PolicyProposals>('/admin/policy-proposals');
  const [creating, setCreating] = useState<Creating>(null);
  const [editing, setEditing] = useState<string | null>(null);
  // A decision or a proposal changes both lists: an approval writes a policy,
  // a switch-off or a deletion outdates or removes proposals.
  const reloadBoth = () => {
    state.reload();
    proposals.reload();
  };
  const done = (changed: boolean) => {
    setCreating(null);
    if (changed) reloadBoth();
  };
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-4">
          {creating ? (
            <Card className="p-4">
              <p className="mb-3 font-medium">
                {creating === 'deny' ? 'Nouvelle politique de refus' : 'Proposer une autorisation'}
              </p>
              {creating === 'deny' ? (
                <PolicyForm data={data} onDone={done} />
              ) : (
                <GrantForm data={data} onDone={done} />
              )}
            </Card>
          ) : (
            <div className="flex flex-wrap gap-2">
              <Button variant="secondary" className="w-fit" onClick={() => setCreating('deny')}>
                <Plus className="size-4" aria-hidden /> Nouvelle politique de refus
              </Button>
              <Button variant="secondary" className="w-fit" onClick={() => setCreating('allow')}>
                <Plus className="size-4" aria-hidden /> Proposer une autorisation
              </Button>
            </div>
          )}
          <ProposalList state={proposals} data={data} onDecided={reloadBoth} />
          <h2 className="font-medium">Politiques</h2>
          {data.policies.length === 0 && <EmptyState title="Aucune politique d’accès" />}
          {data.policies.map((policy) => (
            <PolicyCard
              key={policy.id}
              policy={policy}
              data={data}
              editing={editing === policy.id}
              onEdit={(on) => setEditing(on ? policy.id : null)}
              onChanged={reloadBoth}
            />
          ))}
          <p className="text-xs text-muted">
            Les politiques s’ajoutent aux rôles, sur les permissions que la plateforme vérifie ; les
            permissions d’administration et celles que lisent ses écrans n’en relèvent pas. Un refus
            s’applique dès qu’il est enregistré. Une autorisation, qui élargit l’accès au-delà des
            rôles, est proposée par un administrateur et ne s’applique qu’une fois approuvée par un
            autre ; elle se désactive ou se supprime sans proposition.
          </p>
        </div>
      )}
    </Loaded>
  );
}
