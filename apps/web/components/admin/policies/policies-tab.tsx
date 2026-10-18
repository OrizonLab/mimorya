'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Button, Card, EmptyState, Loaded } from '@/components/ui';
import { useApiData } from '@/lib/api';
import { PolicyCard } from './policy-card';
import { PolicyForm } from './policy-form';
import type { PoliciesView } from './types';

/** Access policies: restrictions applied on top of the roles. */
export function PoliciesTab() {
  const state = useApiData<PoliciesView>('/admin/policies');
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-4">
          {creating ? (
            <Card className="p-4">
              <p className="mb-3 font-medium">Nouvelle politique de refus</p>
              <PolicyForm
                data={data}
                onDone={(changed) => {
                  setCreating(false);
                  if (changed) state.reload();
                }}
              />
            </Card>
          ) : (
            <Button variant="secondary" className="w-fit" onClick={() => setCreating(true)}>
              <Plus className="size-4" aria-hidden /> Nouvelle politique
            </Button>
          )}
          {data.policies.length === 0 && <EmptyState title="Aucune politique d’accès" />}
          {data.policies.map((policy) => (
            <PolicyCard
              key={policy.id}
              policy={policy}
              data={data}
              editing={editing === policy.id}
              onEdit={(on) => setEditing(on ? policy.id : null)}
              onChanged={state.reload}
            />
          ))}
          <p className="text-xs text-muted">
            Les politiques s’ajoutent aux rôles et ne peuvent que restreindre l’accès. Les
            politiques d’autorisation, qui élargiraient l’accès au-delà des rôles, ne se créent pas
            depuis l’administration.
          </p>
        </div>
      )}
    </Loaded>
  );
}
