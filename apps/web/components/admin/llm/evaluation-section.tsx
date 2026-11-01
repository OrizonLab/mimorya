'use client';

import type { FormEvent } from 'react';
import { Button, Card, ErrorNote, Field, Input, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type EvaluationSettingsUpdate,
  type EvaluationSettingsView,
} from '@/lib/api';
import { formatCost } from '@/lib/format';
import { useSyncedState } from '@/lib/use-synced-state';

const SETTINGS = '/admin/evaluation-settings';
const MICROS = 1_000_000;

/** What the company's skill evaluations may cost over thirty days (ADR-154). */
export function EvaluationSection() {
  const state = useApiData<EvaluationSettingsView>(SETTINGS);
  return (
    <Loaded state={state}>{(data) => <EvaluationForm data={data} onSaved={state.reload} />}</Loaded>
  );
}

function EvaluationForm({ data, onSaved }: { data: EvaluationSettingsView; onSaved: () => void }) {
  // Started again from each reloaded setting (ADR-041).
  const [dollars, setDollars] = useSyncedState(String(data.maxCostMicros / MICROS));
  const action = useAction();
  const unchanged = dollars === String(data.maxCostMicros / MICROS);

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = {
      maxCostMicros: Math.round(Number(dollars) * MICROS),
    } satisfies EvaluationSettingsUpdate;
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
        <Field label={`Coût au plus sur ${data.windowDays} jours (USD)`}>
          <Input
            type="number"
            required
            min={data.minCostMicros / MICROS}
            max={data.maxCostCeilingMicros / MICROS}
            step="any"
            value={dollars}
            onChange={(e) => setDollars(e.target.value)}
          />
        </Field>
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
