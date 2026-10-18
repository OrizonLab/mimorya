'use client';

import { useState } from 'react';
import {
  MEMORY_SPACES,
  MemoryProposals,
  MemorySpace,
  PersonalMemoryForm,
  type MemoryItem,
} from '@/components/memory';
import { ErrorNote, Loaded, PageHeader } from '@/components/ui';
import { api, ApiError, useApiData } from '@/lib/api';

export default function MemoryPage() {
  const state = useApiData<{ items: MemoryItem[] }>('/memory');
  const [error, setError] = useState<string | null>(null);

  /** Runs a change, then reloads the list; returns whether it succeeded. */
  async function change(action: () => Promise<unknown>): Promise<boolean> {
    setError(null);
    try {
      await action();
      state.reload();
      return true;
    } catch (e) {
      setError(e instanceof ApiError ? e.message : 'Action impossible');
      return false;
    }
  }

  const review = (id: string, decision: 'approve' | 'reject') =>
    void change(() => api.post(`/memory/${id}/review`, { decision }));
  const remove = (id: string) => void change(() => api.delete(`/memory/${id}`));
  const add = (title: string, content: string) =>
    change(() => api.post('/memory', { space: 'personal', kind: 'preference', title, content }));

  return (
    <div className="mx-auto w-full max-w-4xl px-4 py-8">
      <PageHeader
        title="Mémoire"
        description="Ce que l’agent sait et peut utiliser pour vous aider. Vous pouvez consulter, ajouter et supprimer vos informations à tout moment."
      />
      {error && (
        <div className="mb-4">
          <ErrorNote message={error} />
        </div>
      )}
      <Loaded state={state}>
        {({ items }) => {
          const proposals = items.filter((m) => m.status === 'proposed');
          return (
            <>
              {proposals.length > 0 && <MemoryProposals items={proposals} onReview={review} />}
              {MEMORY_SPACES.map((space) => (
                <MemorySpace
                  key={space.scope}
                  space={space}
                  items={items.filter((m) => m.scope === space.scope && m.status === 'active')}
                  onDelete={remove}
                >
                  {space.scope === 'user' && <PersonalMemoryForm onAdd={add} />}
                </MemorySpace>
              ))}
            </>
          );
        }}
      </Loaded>
    </div>
  );
}
