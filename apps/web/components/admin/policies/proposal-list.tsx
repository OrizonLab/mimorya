'use client';

import { EmptyState, Loaded } from '@/components/ui';
import type { ApiData, Policies, PolicyProposals } from '@/lib/api';
import { ProposalCard } from './proposal-card';

/**
 * The allow policies proposed (ADR-168): those that wait for a decision
 * first, then the latest decided or expired.
 */
export function ProposalList({
  state,
  data,
  onDecided,
}: {
  state: ApiData<PolicyProposals>;
  data: Policies;
  onDecided: () => void;
}) {
  return (
    <section aria-labelledby="proposals-title" className="flex flex-col gap-3">
      <h2 id="proposals-title" className="font-medium">
        Autorisations proposées
      </h2>
      <Loaded state={state}>
        {({ proposals }) =>
          proposals.length === 0 ? (
            <EmptyState title="Aucune autorisation proposée" />
          ) : (
            proposals.map((proposal) => (
              <ProposalCard
                key={proposal.id}
                proposal={proposal}
                data={data}
                onDecided={onDecided}
              />
            ))
          )
        }
      </Loaded>
    </section>
  );
}
