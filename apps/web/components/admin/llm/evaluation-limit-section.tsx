'use client';

import type { FormEvent } from 'react';
import { Button, Card, ErrorNote, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type EvaluationSettingsUpdate,
  type EvaluationSettingsView,
} from '@/lib/api';
import { dollarsOf, formatCost, microsOf } from '@/lib/format';
import { useSyncedState } from '@/lib/use-synced-state';
import { CostField } from './cost-field';

const SETTINGS = '/admin/evaluation-settings';

/** What the company's skill evaluations may cost over thirty days (ADR-154, ADR-155). */
export function EvaluationLimitSection() {
  const state = useApiData<EvaluationSettingsView>(SETTINGS);
  return (
    <Loaded state={state}>
      {(data) => <EvaluationLimitForm data={data} onSaved={state.reload} />}
    </Loaded>
  );
}

function EvaluationLimitForm({
  data,
  onSaved,
}: {
  data: EvaluationSettingsView;
  onSaved: () => void;
}) {
  // Started again from each reloaded setting (ADR-041).
  const [dollars, setDollars] = useSyncedState(dollarsOf(data.maxCostMicros));
  const action = useAction();
  // `2.50` is the `2.5` saved: nothing to save.
  const unchanged = microsOf(dollars) === data.maxCostMicros;

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = { maxCostMicros: microsOf(dollars) } satisfies EvaluationSettingsUpdate;
    if (await action.run(() => api.put(SETTINGS, body))) onSaved();
  }

  async function reset() {
    if (!window.confirm('Revenir à la limite de la plateforme pour les évaluations ?')) return;
    if (await action.run(() => api.delete(SETTINGS))) onSaved();
  }

  return (
    <Card className="p-4">
      <form aria-label="Limite des évaluations" onSubmit={submit} className="flex flex-col gap-3">
        <p className="font-medium">Limite des évaluations des compétences</p>
        <CostField
          label={`Coût au plus sur ${data.windowDays} jours (USD)`}
          dollars={dollars}
          onChange={setDollars}
          minMicros={data.minCostMicros}
          maxMicros={data.maxCostCeilingMicros}
        />
        <p className="text-sm">
          Dépensé sur les {data.windowDays} derniers jours : {formatCost(data.spentMicros)}.
        </p>
        <p className="text-xs text-muted">
          {data.own
            ? 'Limite propre à votre entreprise.'
            : 'Limite de la plateforme, faute de limite propre à votre entreprise.'}{' '}
          Une fois la limite atteinte, aucune évaluation ne peut être demandée, et celle en cours
          s’arrête avant son cas suivant. Plafonds : de {formatCost(data.minCostMicros)} à{' '}
          {formatCost(data.maxCostCeilingMicros)}. Limite de la plateforme :{' '}
          {formatCost(data.platformMaxCostMicros)}.
        </p>
        {action.error && <ErrorNote message={action.error} />}
        <div className="flex flex-wrap gap-2">
          <Button type="submit" loading={action.pending} disabled={unchanged}>
            Enregistrer
          </Button>
          {data.own && (
            <Button type="button" variant="secondary" onClick={() => void reset()}>
              Revenir à la limite de la plateforme
            </Button>
          )}
        </div>
      </form>
    </Card>
  );
}
