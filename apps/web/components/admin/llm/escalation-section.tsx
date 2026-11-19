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
import { dollarsOf, formatCost, formatDate, microsOf, PLATFORM_TIME_ZONE } from '@/lib/format';
import { useSyncedState } from '@/lib/use-synced-state';
import { CostField } from './cost-field';
import { MAIN_ROUTES, RouterRoutesFields } from './router-routes';

const ESCALATION = '/admin/model-escalation';

/** A stronger model of the provider of the company's runs (its own, or the
 * platform's in the formulas the operator gives it, ADR-209), called by the
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
  const [routes, setRoutes] = useSyncedState(settings?.routes ?? MAIN_ROUTES);
  const action = useAction();
  const served = data.mainModel !== null;
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
      routes,
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
          clé (celle de votre entreprise, ou celle de l’exploitant si elle utilise le modèle de la
          plateforme), n’intervient que quand un mécanisme activé le demande. Chaque appel compte
          dans un budget mensuel (USD) et dans un nombre d’escalades par run ; au-delà, le run
          continue avec le modèle principal. Seul un modèle dont la plateforme connaît le prix peut
          servir (aujourd’hui, ceux d’Anthropic) : sans prix, le budget ne pourrait pas borner ses
          appels. Après un échec du contrôle des réponses, une fois les corrections du modèle
          principal épuisées, le modèle d’escalade reprend le run là où il en est, une fois par run
          ; les actions déjà faites ne sont pas refaites, et les validations restent les mêmes. Le
          routeur choisit le modèle au début du run, selon sa sorte.
        </p>
        {!served && (
          <p className="text-sm">
            L’escalade demande un modèle pour vos conversations : le fournisseur propre de votre
            entreprise, ou le modèle de la plateforme que l’exploitant lui fournit.
          </p>
        )}
        {data.onOperatorKey && (
          <p className="text-sm">
            Votre entreprise utilise le modèle de la plateforme : chaque appel d’escalade compte
            dans ce budget et dans le plafond ou le crédit de sa formule, à son montant refacturé,
            sans être facturé deux fois.
          </p>
        )}
        {data.problem && <ErrorNote message={data.problem} />}
        <fieldset disabled={!served} className="flex flex-col gap-3">
          <Field
            label={
              served
                ? `Modèle d’escalade (modèle principal : ${data.mainModel})`
                : 'Modèle d’escalade'
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
                <div key={mechanism.key} className="flex flex-col gap-2">
                  <label className="flex items-center gap-2 text-sm">
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
                  {mechanism.key === 'router' && on && (
                    <RouterRoutesFields routes={routes} onChange={setRoutes} />
                  )}
                  {mechanism.key === 'advisor' && on && (
                    <p className="text-xs text-muted">
                      Le modèle principal consulte le modèle d’escalade en cours de réponse, chez
                      Anthropic : chaque consultation compte comme une escalade, dans le budget du
                      mois et les escalades permises par run.
                    </p>
                  )}
                </div>
              );
            })}
          </div>
        </fieldset>
        <p className="text-sm">
          Dépensé par l’escalade depuis le {formatDate(data.monthStart, PLATFORM_TIME_ZONE)} :{' '}
          {formatCost(data.spentMicros)}
          {settings && ` sur ${formatCost(settings.monthlyBudgetMicros)}`}.
        </p>
        {action.error && <ErrorNote message={action.error} />}
        <div className="flex flex-wrap gap-2">
          <Button type="submit" loading={action.pending} disabled={!served}>
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
