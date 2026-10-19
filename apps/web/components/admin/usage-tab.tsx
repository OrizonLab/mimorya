'use client';

import { Card, Cell, DataTable, EmptyState, Loaded } from '@/components/ui';
import { useApiData, type UsageSummary } from '@/lib/api';
import { formatCost, formatNumber } from '@/lib/format';

export function UsageTab() {
  const state = useApiData<UsageSummary>('/admin/usage?days=30');
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-4">
          <Card className="p-4">
            <p className="text-xs text-muted">Coût estimé sur 30 jours</p>
            <p className="mt-1 text-2xl font-semibold">
              {formatCost(data.rows.reduce((sum, r) => sum + r.costMicros, 0))}
            </p>
            <p className="mt-1 text-xs text-muted">
              Estimation à partir des prix publics des modèles ; les modèles non tarifés comptent
              pour 0.
            </p>
          </Card>
          {data.rows.length === 0 ? (
            <EmptyState title="Aucune consommation sur la période" />
          ) : (
            <DataTable columns={['Modèle', 'Appels', 'Tokens entrée', 'Tokens sortie', 'Coût']}>
              {data.rows.map((r) => (
                <tr key={`${r.provider}:${r.model}:${r.kind}`}>
                  <Cell>
                    {r.model} <span className="text-xs text-muted">({r.provider})</span>
                  </Cell>
                  <Cell>{r.calls}</Cell>
                  <Cell>{formatNumber(r.inputTokens)}</Cell>
                  <Cell>{formatNumber(r.outputTokens)}</Cell>
                  <Cell>{formatCost(r.costMicros)}</Cell>
                </tr>
              ))}
            </DataTable>
          )}
        </div>
      )}
    </Loaded>
  );
}
