'use client';

import type { FormEvent } from 'react';
import { Button, Card, ErrorNote, Field, Input, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type MissionSettingsUpdate,
  type MissionSettingsView,
} from '@/lib/api';
import { dollarsOf, formatCost, microsOf } from '@/lib/format';
import { useSyncedState } from '@/lib/use-synced-state';
import { CostField } from './cost-field';

const SETTINGS = '/admin/mission-settings';

/** The limits of the missions the company's people entrust (ADR-093). */
export function MissionSection() {
  const state = useApiData<MissionSettingsView>(SETTINGS);
  return (
    <Loaded state={state}>{(data) => <MissionForm data={data} onSaved={state.reload} />}</Loaded>
  );
}

/** The limits as the form writes them: stages, and dollars. */
function draftOf(data: MissionSettingsView) {
  return { stages: String(data.maxStages), dollars: dollarsOf(data.maxCostMicros) };
}

function MissionForm({ data, onSaved }: { data: MissionSettingsView; onSaved: () => void }) {
  // Started again from each reloaded setting (ADR-041).
  const [draft, setDraft] = useSyncedState(draftOf(data));
  const action = useAction();
  // `2.50` is the `2.5` saved, `06` the `6`: nothing to save.
  const unchanged =
    Number(draft.stages) === data.maxStages && microsOf(draft.dollars) === data.maxCostMicros;

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = {
      maxStages: Number(draft.stages),
      maxCostMicros: microsOf(draft.dollars),
    } satisfies MissionSettingsUpdate;
    if (await action.run(() => api.put(SETTINGS, body))) onSaved();
  }

  async function reset() {
    if (!window.confirm('Revenir aux limites de la plateforme pour les prochaines missions ?')) {
      return;
    }
    if (await action.run(() => api.delete(SETTINGS))) onSaved();
  }

  return (
    <Card className="p-4">
      <form aria-label="Limites des missions" onSubmit={submit} className="flex flex-col gap-3">
        <p className="font-medium">Limites des missions</p>
        <div className="flex flex-wrap gap-3">
          <Field label="Étapes au plus par mission">
            <Input
              type="number"
              required
              min={1}
              max={data.maxStagesCeiling}
              step={1}
              value={draft.stages}
              onChange={(e) => setDraft({ ...draft, stages: e.target.value })}
            />
          </Field>
          <CostField
            label="Coût au plus par mission (USD)"
            dollars={draft.dollars}
            onChange={(dollars) => setDraft({ ...draft, dollars })}
            minMicros={data.minCostMicros}
            maxMicros={data.maxCostCeilingMicros}
          />
        </div>
        <p className="text-xs text-muted">
          {data.own
            ? 'Limites propres à votre entreprise.'
            : 'Limites de la plateforme, faute de limites propres à votre entreprise.'}{' '}
          Une mission s’arrête inachevée une fois ses étapes ou son coût épuisés ; elle garde les
          limites du jour où elle est confiée. Plafonds : {data.maxStagesCeiling} étapes, de{' '}
          {formatCost(data.minCostMicros)} à {formatCost(data.maxCostCeilingMicros)}. Limites de la
          plateforme : {data.platformMaxStages} étapes, {formatCost(data.platformMaxCostMicros)}.
        </p>
        {action.error && <ErrorNote message={action.error} />}
        <div className="flex flex-wrap gap-2">
          <Button type="submit" loading={action.pending} disabled={unchanged}>
            Enregistrer
          </Button>
          {data.own && (
            <Button type="button" variant="secondary" onClick={() => void reset()}>
              Revenir aux limites de la plateforme
            </Button>
          )}
        </div>
      </form>
    </Card>
  );
}
