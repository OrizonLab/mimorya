'use client';

import { Card, Cell, DataTable, EmptyState, Loaded } from '@/components/ui';
import { useApiData, type JevPrice, type UsageRow, type UsageSummary } from '@/lib/api';
import { formatCost, formatDate, formatNumber, PLATFORM_TIME_ZONE } from '@/lib/format';
import { USAGE_KIND_LABELS, USAGE_ROLE_LABELS } from '@/lib/labels';

/** What the company's use of models cost it: per model, with the role each
 * call played and the escalation's budget for the month (ADR-207); on the
 * platform's model, the amounts the operator re-invoices (ADR-210); and
 * Jev's dated price, where the operator opens it (ADR-213). */
export function UsageTab() {
  const state = useApiData<UsageSummary>('/admin/usage?days=30');
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-4">
          <Card className="p-4">
            <p className="text-xs text-muted">Coût sur 30 jours</p>
            <p className="mt-1 text-2xl font-semibold">
              {formatCost(data.rows.reduce((sum, r) => sum + r.costMicros, 0))}
            </p>
            <p className="mt-1 text-xs text-muted">
              Sur votre propre clé, estimation à partir des prix publics des modèles ; les modèles
              non tarifés comptent pour 0.
              {data.rows.some((r) => r.onOperatorKey) &&
                ' Sur le modèle de la plateforme, montants refacturés par l’exploitant.'}
            </p>
          </Card>
          {data.escalation && (
            <Card className="p-4">
              <p className="text-xs text-muted">
                Escalade de modèle depuis le{' '}
                {formatDate(data.escalation.monthStart, PLATFORM_TIME_ZONE)}
              </p>
              <p className="mt-1 text-lg font-semibold">
                {formatCost(data.escalation.spentMicros)} sur{' '}
                {formatCost(data.escalation.budgetMicros)}
              </p>
              <p className="mt-1 text-xs text-muted">
                Appels au modèle d’escalade, classements et consultations du conseiller ; une fois
                le budget atteint, les runs gardent le modèle principal.
              </p>
            </Card>
          )}
          {data.jevPrices.length > 0 && <JevPrices prices={data.jevPrices} />}
          {data.rows.length === 0 ? (
            <EmptyState title="Aucune consommation sur la période" />
          ) : (
            <>
              <RoleBreakdown rows={data.rows} />
              <DataTable
                columns={['Modèle', 'Rôle', 'Appels', 'Tokens entrée', 'Tokens sortie', 'Coût']}
              >
                {data.rows.map((r) => (
                  <tr key={`${r.provider}:${r.model}:${r.kind}:${r.role}:${r.onOperatorKey}`}>
                    <Cell>
                      {r.model}{' '}
                      <span className="text-xs text-muted">
                        ({r.provider}
                        {r.onOperatorKey && ', modèle de la plateforme'})
                      </span>
                    </Cell>
                    <Cell>{roleOf(r)}</Cell>
                    <Cell>{r.calls}</Cell>
                    <Cell>{formatNumber(r.inputTokens)}</Cell>
                    <Cell>{formatNumber(r.outputTokens)}</Cell>
                    <Cell>{formatCost(r.costMicros)}</Cell>
                  </tr>
                ))}
              </DataTable>
            </>
          )}
        </div>
      )}
    </Loaded>
  );
}

function roleOf(row: UsageRow): string {
  return row.kind === 'llm' ? USAGE_ROLE_LABELS[row.role] : USAGE_KIND_LABELS[row.kind];
}

/** What each role of the model calls cost over the period. */
function RoleBreakdown({ rows }: { rows: UsageRow[] }) {
  const costs = new Map<string, number>();
  for (const row of rows) costs.set(roleOf(row), (costs.get(roleOf(row)) ?? 0) + row.costMicros);
  return (
    <Card className="p-4">
      <p className="text-xs text-muted">Répartition par rôle</p>
      <ul aria-label="Répartition par rôle" className="mt-2 flex flex-wrap gap-x-6 gap-y-1 text-sm">
        {[...costs].map(([role, cost]) => (
          <li key={role}>
            {role} : <span className="font-medium">{formatCost(cost)}</span>
          </li>
        ))}
      </ul>
    </Card>
  );
}

const PER_MTOK = new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 3 });

/** Jev's price per million tokens, as read on its date at its source. */
function JevPrices({ prices }: { prices: JevPrice[] }) {
  const [first] = prices;
  if (!first) return null;
  return (
    <Card className="p-4">
      <p className="text-xs text-muted">
        Tarif de Jev (TypeSafe), relevé le {first.readOn.split('-').reverse().join('/')}
      </p>
      <ul aria-label="Tarif de Jev" className="mt-2 flex flex-col gap-1 text-sm">
        {prices.map((price) => (
          <li key={price.model}>
            {price.model} : {PER_MTOK.format(price.inputPerMtok)} $US par million de tokens en
            entrée
            {price.outputPerMtok === 0
              ? ', sortie gratuite'
              : `, ${PER_MTOK.format(price.outputPerMtok)} $US en sortie`}
          </li>
        ))}
      </ul>
      <p className="mt-1 text-xs text-muted">
        Source : {first.source}. Les appels à Jev sont faits avec la clé de votre entreprise : ils
        sont comptés au rôle « Décision ».
      </p>
    </Card>
  );
}
