'use client';

import { Button, Card, Checkbox, ErrorNote } from '@/components/ui';
import {
  api,
  useAction,
  type ApprovalRoutingUpdate,
  type ApprovalRoutingView,
  type ToolRisk,
} from '@/lib/api';
import { RISK_LABELS } from '@/lib/labels';
import { useSyncedState } from '@/lib/use-synced-state';

/** Every risk, from the lightest to the heaviest, as the API orders them. */
const RISKS = Object.keys(RISK_LABELS) as ToolRisk[];

/** Chooses the risks whose actions a manager validates. */
export function ApprovalRoutingForm({
  data,
  onSaved,
}: {
  data: ApprovalRoutingView;
  onSaved: () => void;
}) {
  // Started again from each reloaded setting (ADR-041).
  const [chosen, setChosen] = useSyncedState(data.managerRisks);
  const action = useAction();
  const changed = RISKS.some((r) => chosen.includes(r) !== data.managerRisks.includes(r));

  function toggle(risk: ToolRisk) {
    setChosen((current) =>
      current.includes(risk) ? current.filter((r) => r !== risk) : [...current, risk],
    );
  }

  async function save() {
    const body = {
      managerRisks: RISKS.filter((r) => chosen.includes(r)),
    } satisfies ApprovalRoutingUpdate;
    if (await action.run(() => api.put('/admin/approval-routing', body))) onSaved();
  }

  return (
    <Card className="flex flex-col gap-3 p-4">
      <p className="font-medium">Actions validées par un responsable</p>
      <p className="text-sm">
        Les actions des risques cochés sont validées par le responsable de la personne pour qui
        l’agent agit, plutôt que par elle : le responsable d’unité le plus proche au-dessus d’elle,
        dont le compte est actif et qui détient sur elle la permission « Valider les actions de son
        périmètre ». La position seule n’accorde rien : si personne n’est habilité, l’action est
        refusée aussitôt. La personne peut toujours retirer sa demande.
      </p>
      <fieldset className="flex flex-col gap-2">
        <legend className="sr-only">Risques validés par un responsable</legend>
        {RISKS.map((risk) => (
          <label key={risk} className="flex items-center gap-2 text-sm">
            <Checkbox checked={chosen.includes(risk)} onChange={() => toggle(risk)} />
            {RISK_LABELS[risk].label}
          </label>
        ))}
      </fieldset>
      <p className="text-xs text-muted">
        Les risques « Lecture » et « Brouillon » ne demandent une validation que pour les outils qui
        l’exigent toujours. Le responsable n’est pas encore prévenu par e-mail : il trouve les
        demandes dans « Validations ». Aucune case cochée : chacun valide ses propres actions.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <Button className="w-fit" loading={action.pending} disabled={!changed} onClick={save}>
        Enregistrer
      </Button>
    </Card>
  );
}
