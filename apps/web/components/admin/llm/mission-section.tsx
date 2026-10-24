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
import { formatCost } from '@/lib/format';
import { useSyncedState } from '@/lib/use-synced-state';

const SETTINGS = '/admin/mission-settings';
const MICROS = 1_000_000;

/** The limits of the missions the company's people entrust (ADR-093). */
export function MissionSection() {
  const state = useApiData<MissionSettingsView>(SETTINGS);
  return (
    <Loaded state={state}>{(data) => <MissionForm data={data} onSaved={state.reload} />}</Loaded>
  );
}

/** The limits as the form writes them: stages, and dollars. */
function draftOf(data: MissionSettingsView) {
  return { stages: String(data.maxStages), dollars: String(data.maxCostMicros / MICROS) };
}

function MissionForm({ data, onSaved }: { data: MissionSettingsView; onSaved: () => void }) {
  // Started again from each reloaded setting (ADR-041).
  const [draft, setDraft] = useSyncedState(draftOf(data));
  const action = useAction();
  const unchanged = JSON.stringify(draft) === JSON.stringify(draftOf(data));

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = {
      maxStages: Number(draft.stages),
      maxCostMicros: Math.round(Number(draft.dollars) * MICROS),
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
          <Field label="Coût au plus par mission (USD)">
            <Input
              type="number"
              required
              min={data.minCostMicros / MICROS}
              max={data.maxCostCeilingMicros / MICROS}
              step="any"
              value={draft.dollars}
              onChange={(e) => setDraft({ ...draft, dollars: e.target.value })}
            />
          </Field>
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
