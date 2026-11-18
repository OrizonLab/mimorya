'use client';

import type { FormEvent } from 'react';
import { Button, Card, Checkbox, ErrorNote, Field, Input, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type EscalationMechanism,
  type ModelEscalationUpdate,
  type ModelEscalationView,
} from '@/lib/api';
import { dollarsOf, formatCost, formatDate, microsOf } from '@/lib/format';
import { useSyncedState } from '@/lib/use-synced-state';
import { CostField } from './cost-field';

const ESCALATION = '/admin/model-escalation';

/** A stronger model of the conversations' provider, called by the
 * mechanisms the company turns on, within a monthly budget (ADR-207). */
export function EscalationSection() {
  const state = useApiData<ModelEscalationView>(ESCALATION);
  return (
    <Loaded state={state}>{(data) => <EscalationForm data={data} onSaved={state.reload} />}</Loaded>
  );
}

function EscalationForm({ data, onSaved }: { data: ModelEscalationView; onSaved: () => void }) {
  const settings = data.settings;
  // Started again from each reloaded setting (ADR-041).
  const [model, setModel] = useSyncedState(settings?.model ?? '');
  const [dollars, setDollars] = useSyncedState(
    settings ? dollarsOf(settings.monthlyBudgetMicros) : '',
  );
  const [perRun, setPerRun] = useSyncedState(String(settings?.maxPerRun ?? 5));
  const [afterFailure, setAfterFailure] = useSyncedState(settings?.afterFailure ?? false);
  const [router, setRouter] = useSyncedState(settings?.router ?? false);
  const [advisor, setAdvisor] = useSyncedState(settings?.advisor ?? false);
  const action = useAction();
  const own = data.mainModel !== null;
  const turnedOn: Record<EscalationMechanism, [boolean, (on: boolean) => void]> = {
    afterFailure: [afterFailure, setAfterFailure],
    router: [router, setRouter],
    advisor: [advisor, setAdvisor],
  };

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = {
      model,
      afterFailure,
      router,
      advisor,
      monthlyBudgetMicros: microsOf(dollars),
      maxPerRun: Number(perRun),
    } satisfies ModelEscalationUpdate;
    if (await action.run(() => api.put(ESCALATION, body))) onSaved();
  }

  async function remove() {
    if (!window.confirm('Retirer l’escalade de modèle ? Vos runs garderont le modèle principal.'))
      return;
    if (await action.run(() => api.delete(ESCALATION))) onSaved();
  }

  return (
    <Card className="p-4">
      <form aria-label="Escalade de modèle" onSubmit={submit} className="flex flex-col gap-3">
        <p className="font-medium">Escalade de modèle</p>
        <p className="text-xs text-muted">
          Un modèle plus puissant, chez le même fournisseur que vos conversations et avec la même
          clé, n’intervient que quand un mécanisme activé le demande. Chaque appel compte dans un
          budget mensuel (USD) et dans un nombre d’escalades par run ; au-delà, le run continue avec
          le modèle principal.
        </p>
        {!own && (
          <p className="text-sm">
            L’escalade demande le fournisseur propre de votre entreprise, qui en paie les appels :
            choisissez d’abord celui de vos conversations.
          </p>
        )}
        {data.problem && <ErrorNote message={data.problem} />}
        <fieldset disabled={!own} className="flex flex-col gap-3">
          <Field
            label={
              own ? `Modèle d’escalade (modèle principal : ${data.mainModel})` : 'Modèle d’escalade'
            }
          >
            <Input
              required
              value={model}
              onChange={(e) => setModel(e.target.value)}
              placeholder="claude-opus-5-5"
            />
          </Field>
          <CostField
            label="Budget mensuel d’escalade (USD)"
            dollars={dollars}
            onChange={setDollars}
            minMicros={data.minBudgetMicros}
            maxMicros={data.maxBudgetMicros}
          />
          <Field label="Escalades au plus par run">
            <Input
              type="number"
              required
              min={1}
              max={data.maxPerRunCeiling}
              step={1}
              value={perRun}
              onChange={(e) => setPerRun(e.target.value)}
            />
          </Field>
          <div className="flex flex-col gap-2">
            {data.mechanisms.map((mechanism) => {
              const [on, setOn] = turnedOn[mechanism.key];
              return (
                <label key={mechanism.key} className="flex items-center gap-2 text-sm">
                  <Checkbox
                    checked={on}
                    disabled={!mechanism.available && !on}
                    onChange={(e) => setOn(e.target.checked)}
                  />
                  {mechanism.label}
                  {mechanism.reason && (
                    <span className="text-xs text-muted">({mechanism.reason})</span>
                  )}
                </label>
              );
            })}
          </div>
        </fieldset>
        <p className="text-sm">
          Dépensé par l’escalade depuis le {formatDate(data.monthStart)} :{' '}
          {formatCost(data.spentMicros)}
          {settings && ` sur ${formatCost(settings.monthlyBudgetMicros)}`}.
        </p>
        {action.error && <ErrorNote message={action.error} />}
        <div className="flex flex-wrap gap-2">
          <Button type="submit" loading={action.pending} disabled={!own}>
            Enregistrer
          </Button>
          {settings && (
            <Button type="button" variant="secondary" onClick={() => void remove()}>
              Retirer l’escalade
            </Button>
          )}
        </div>
      </form>
    </Card>
  );
}
