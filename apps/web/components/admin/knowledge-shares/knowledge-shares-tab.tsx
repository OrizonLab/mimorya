'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Button, Card, EmptyState, Loaded } from '@/components/ui';
import { useApiData, type KnowledgeShares } from '@/lib/api';
import { ShareCard } from './share-card';
import { ShareForm } from './share-form';

/** A unit's knowledge shared with other units (ADR-143): listed, made, removed. */
export function KnowledgeSharesTab() {
  const state = useApiData<KnowledgeShares>('/admin/knowledge-shares');
  const [creating, setCreating] = useState(false);
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-4">
          {creating ? (
            <Card className="p-4">
              <p className="mb-3 font-medium">Nouveau partage</p>
              <ShareForm
                units={data.units}
                onDone={(changed) => {
                  setCreating(false);
                  if (changed) state.reload();
                }}
              />
            </Card>
          ) : (
            <Button variant="secondary" className="w-fit" onClick={() => setCreating(true)}>
              <Plus className="size-4" aria-hidden /> Nouveau partage
            </Button>
          )}
          {data.shares.length === 0 && <EmptyState title="Aucun partage de connaissances" />}
          {data.shares.map((share) => (
            <ShareCard key={share.id} share={share} units={data.units} onChanged={state.reload} />
          ))}
          <p className="text-xs text-muted">
            Les membres de l’unité qui reçoit un partage lisent la mémoire et les documents de
            l’unité partagée comme ceux de la leur, jamais au-delà de la sensibilité choisie ni de
            leur propre habilitation, et sans pouvoir les modifier ni les supprimer. Seul l’espace
            de l’unité partagée s’ouvre, non celui des unités en dessous, et jamais la mémoire
            personnelle de quiconque. Un administrateur ne fait pas un partage dont il serait
            lui-même lecteur : un autre administrateur s’en charge.
          </p>
        </div>
      )}
    </Loaded>
  );
}
