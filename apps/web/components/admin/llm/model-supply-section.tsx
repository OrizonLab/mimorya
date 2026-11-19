'use client';

import { Card, Cell, DataTable, ErrorNote, Loaded } from '@/components/ui';
import { useApiData, type ModelSupplyView, type SupplyFormulaView } from '@/lib/api';
import { formatCost, formatDate, PLATFORM_TIME_ZONE } from '@/lib/format';

const FORMULAS: Record<SupplyFormulaView, string> = {
  clientKey: 'Clé de votre entreprise : l’exploitant ne fournit pas de modèle',
  capped: 'Modèle de la plateforme, avec un plafond mensuel',
  prepaid: 'Modèle de la plateforme, sur un crédit prépayé',
};

/** How the operator supplies the company's model (ADR-208, ADR-210): its
 * formula, its cap and what the month cost under it, or its credit's
 * balance and payments; what the company pays, which the operator alone
 * sets. */
export function ModelSupplySection() {
  const state = useApiData<ModelSupplyView>('/admin/model-supply');
  return <Loaded state={state}>{(data) => <ModelSupply data={data} />}</Loaded>;
}

function ModelSupply({ data }: { data: ModelSupplyView }) {
  const since = formatDate(data.monthStart, PLATFORM_TIME_ZONE);
  return (
    <Card className="p-4">
      <section aria-label="Fourniture du modèle" className="flex flex-col gap-3">
        <p className="font-medium">Fourniture du modèle</p>
        <p className="text-sm">{FORMULAS[data.formula]}.</p>
        {data.formula !== 'clientKey' && !data.runsOnOperatorKey && (
          <p className="text-xs text-muted">
            Vos conversations utilisent votre propre fournisseur : la formule de l’exploitant ne
            sert pas tant que vous le gardez.
          </p>
        )}
        {data.formula === 'capped' && (
          <p className="text-sm">
            {data.unlimited || data.monthlyCapMicros === null
              ? `Sans plafond, à la demande de l’exploitant : ${formatCost(data.monthBilledMicros)} depuis le ${since}.`
              : `Consommé depuis le ${since} : ${formatCost(data.monthBilledMicros)} sur ${formatCost(data.monthlyCapMicros)}.`}
          </p>
        )}
        {data.balanceMicros !== null && (
          <p className="text-sm">
            Solde du crédit :{' '}
            <span className="font-semibold">{formatCost(data.balanceMicros)}</span>
          </p>
        )}
        {data.stopped && <ErrorNote message={data.stopped} />}
        {data.payments.length > 0 && (
          <DataTable columns={['Date', 'Référence', 'Montant']}>
            {data.payments.map((payment) => (
              <tr key={payment.reference}>
                <Cell>{formatDate(payment.recordedAt)}</Cell>
                <Cell>{payment.reference}</Cell>
                <Cell>{formatCost(payment.amountMicros)}</Cell>
              </tr>
            ))}
          </DataTable>
        )}
        <p className="text-xs text-muted">
          La formule, le plafond et les versements sont réglés par l’exploitant de la plateforme.
          Les montants, en USD, sont ceux qu’il vous refacture ; au-delà du plafond ou du crédit,
          aucun run ne part sur le modèle de la plateforme.
        </p>
      </section>
    </Card>
  );
}
