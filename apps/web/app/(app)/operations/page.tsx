'use client';

import { RunList } from '@/components/operations';
import { EmptyState, Loaded, PageHeader } from '@/components/ui';
import { useApiData, type RunList as Runs } from '@/lib/api';

export default function OperationsPage() {
  const state = useApiData<Runs>('/runs');
  return (
    <div className="mx-auto w-full max-w-5xl px-4 py-8">
      <PageHeader
        title="Opérations"
        description="Les travaux réalisés par l’agent pour vous : étapes, outils utilisés, validations, coût. Tout est tracé."
      />
      <Loaded state={state}>
        {({ runs }) =>
          runs.length === 0 ? (
            <EmptyState title="Aucune opération pour le moment" />
          ) : (
            <RunList runs={runs} />
          )
        }
      </Loaded>
    </div>
  );
}
