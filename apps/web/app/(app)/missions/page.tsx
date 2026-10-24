'use client';

import { useEffect } from 'react';
import { MissionCard, MissionForm, underWay } from '@/components/missions';
import { Card, EmptyState, Loaded, PageHeader } from '@/components/ui';
import { useApiData, type MissionList } from '@/lib/api';
import { useCan } from '@/lib/me-context';

/** How often the list is read again while a mission is under way. */
const REFRESH_MS = 10_000;

export default function MissionsPage() {
  const state = useApiData<MissionList>('/missions');
  const mayCreate = useCan('mission:create');
  const mayUseAgent = useCan('agent:use');
  const mayEntrust = mayCreate && mayUseAgent;
  const going = state.data?.missions.some(underWay) ?? false;
  // At its limit, the reader learns it before writing a mission (the API checks anyway).
  const full =
    state.data !== null && state.data.missions.filter(underWay).length >= state.data.maxUnderWay;
  const reload = state.reload;

  // Its stages go on in the background: the list follows them.
  useEffect(() => {
    if (!going) return;
    const timer = window.setInterval(reload, REFRESH_MS);
    return () => window.clearInterval(timer);
  }, [going, reload]);

  return (
    <div className="mx-auto w-full max-w-4xl px-4 py-8">
      <PageHeader
        title="Missions"
        description="Un travail plus long que l’agent mène en arrière-plan, étape par étape, avec vos droits du moment. Chaque étape a un budget limité ; à la fin de chacune, l’agent écrit un point d’étape dont repart la suivante. Sa réponse finale est vérifiée au regard des critères de réussite. Les actions qui demandent une validation vous attendent dans « Validations »."
      />
      <section aria-labelledby="new-mission" className="mb-6">
        <Card className="flex flex-col gap-3 p-4">
          <h2 id="new-mission" className="font-medium">
            Nouvelle mission
          </h2>
          {!mayEntrust ? (
            <p className="text-sm text-muted">
              Vous n’avez pas le droit de confier des missions à l’agent.
            </p>
          ) : full ? (
            <p className="text-sm text-muted">
              Vous avez déjà {state.data?.maxUnderWay} missions en cours : attendez la fin de l’une
              d’elles, ou arrêtez-la.
            </p>
          ) : (
            <MissionForm onEntrusted={reload} />
          )}
        </Card>
      </section>
      <Loaded state={state}>
        {({ missions }) =>
          missions.length === 0 ? (
            <EmptyState title="Aucune mission" />
          ) : (
            <div className="flex flex-col gap-3">
              {missions.map((mission) => (
                <MissionCard key={mission.id} mission={mission} onChanged={reload} />
              ))}
            </div>
          )
        }
      </Loaded>
    </div>
  );
}
