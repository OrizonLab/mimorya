'use client';

import { useState, type FormEvent } from 'react';
import { Button, Card, ErrorNote, Loaded, Radio } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type DecisionEngineChoice,
  type DecisionSettingsUpdate,
  type DecisionSettingsView,
  type DecisionTestResult,
} from '@/lib/api';
import { formatDate } from '@/lib/format';
import { useSyncedState } from '@/lib/use-synced-state';
import { DecisionUsagesFields, usagesOf } from './decision-usages';
import { KeyField } from './key-field';

const DECISIONS = '/admin/decisions';

/** The company's decision service (ADR-213): quick, typed judgements that
 * can only make the assistant more careful, asked of its main model or, where
 * the operator opens it, of TypeSafe's Jev. */
export function DecisionSection() {
  const state = useApiData<DecisionSettingsView>(DECISIONS);
  return (
    <Loaded state={state}>{(data) => <DecisionForm data={data} onSaved={state.reload} />}</Loaded>
  );
}

function DecisionForm({ data, onSaved }: { data: DecisionSettingsView; onSaved: () => void }) {
  // Started again from each reloaded setting (ADR-041).
  const [engine, setEngine] = useSyncedState<DecisionEngineChoice>(data.engine);
  const [usages, setUsages] = useSyncedState(usagesOf(data.usages));
  const [key, setKey] = useState('');
  const [tested, setTested] = useState<DecisionTestResult | null>(null);
  const action = useAction();
  const jev = engine === 'jev';

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = {
      engine,
      jevApiKey: key.trim() || undefined,
      usages,
    } satisfies DecisionSettingsUpdate;
    if (await action.run(() => api.put(DECISIONS, body))) {
      setKey('');
      setTested(null);
      onSaved();
    }
  }

  async function test() {
    setTested(await action.run(() => api.post<DecisionTestResult>(`${DECISIONS}/test`)));
  }

  async function remove() {
    const asked =
      'Revenir au modèle principal, tous les usages désactivés ? La clé TypeSafe sera effacée.';
    if (!window.confirm(asked)) return;
    if (await action.run(() => api.delete(DECISIONS))) {
      setTested(null);
      onSaved();
    }
  }

  return (
    <Card className="p-4">
      <form aria-label="Service de décision" onSubmit={submit} className="flex flex-col gap-3">
        <p className="font-medium">Service de décision</p>
        <p className="text-xs text-muted">
          Des jugements rapides et typés (oui ou non, un choix, une note) que l’assistant demande à
          certaines étapes. Une décision ne peut que le rendre plus prudent : ajouter une validation
          humaine, bloquer, signaler, ne pas garder ; elle ne lève jamais une permission, une
          validation ni un refus. En cas d’erreur ou de délai dépassé, l’usage applique sa valeur
          prudente. Seuls les champs nécessaires à chaque usage sont envoyés, jamais toute la
          conversation ni un secret. En observation, la décision est prise et journalisée sans
          effet, pour régler son seuil. Le test interroge le moteur enregistré, sans repli.
        </p>
        <fieldset className="flex flex-col gap-2">
          <legend className="mb-2 text-sm font-medium">Moteur</legend>
          <label className="flex items-center gap-2 text-sm">
            <Radio
              name="decision-engine"
              checked={engine === 'mainModel'}
              onChange={() => setEngine('mainModel')}
            />
            Modèle principal{data.mainModel ? ` (${data.mainModel})` : ''}, par défaut : les
            décisions restent chez le fournisseur de vos conversations
          </label>
          <label className="flex items-center gap-2 text-sm">
            <Radio
              name="decision-engine"
              checked={jev}
              disabled={!data.jevAvailable && !jev}
              onChange={() => setEngine('jev')}
            />
            Jev de TypeSafe ({data.jevModel})
            {data.jevReason && <span className="text-xs text-muted">({data.jevReason})</span>}
          </label>
        </fieldset>
        <p className="text-xs text-muted">
          Le modèle principal, choisi ou en repli quand Jev ne répond pas, dit sa certitude (faible,
          moyenne ou forte), traduite en probabilité par une table fixe : cette probabilité n’est
          pas calibrée, et un seuil réglé pour Jev ne vaut pas pour lui.
        </p>
        {jev && (
          <>
            <p className="text-sm">
              TypeSafe est un sous-traitant situé hors de l’Union européenne : l’état minimal de
              chaque décision lui est envoyé. Avant de le choisir, signez son accord de traitement
              des données et demandez-lui la conservation nulle (« zero data retention »). Ses
              appels sont faits avec la clé de votre entreprise et comptés dans la consommation.
            </p>
            <KeyField of="TypeSafe" required keeps={data.hasJevKey} value={key} onChange={setKey} />
          </>
        )}
        {data.problem && <ErrorNote message={data.problem} />}
        <DecisionUsagesFields usages={data.usages} values={usages} onChange={setUsages} />
        {data.updatedAt && (
          <p className="text-xs text-muted">Modifié le {formatDate(data.updatedAt)}.</p>
        )}
        {tested && (
          <p className="text-sm text-ok">
            {`${tested.model} a répondu en ${tested.latencyMs} ms : le message d’essai est une salutation avec une probabilité de ${Math.round(tested.yes * 100)} %${tested.engine === 'mainModel' ? ' (déduite de sa certitude, non calibrée)' : ''}.`}
          </p>
        )}
        {action.error && <ErrorNote message={action.error} />}
        <div className="flex flex-wrap gap-2">
          <Button type="submit" loading={action.pending}>
            Enregistrer
          </Button>
          <Button type="button" variant="secondary" onClick={() => void test()}>
            Tester le moteur
          </Button>
          {data.updatedAt && (
            <Button type="button" variant="secondary" onClick={() => void remove()}>
              Revenir aux réglages par défaut
            </Button>
          )}
        </div>
      </form>
    </Card>
  );
}
