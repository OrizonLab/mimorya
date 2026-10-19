'use client';

import { Cell, DataTable, LabelBadge, Loaded, type ToneLabel } from '@/components/ui';
import { useApiData, type AuditActor, type AuditLog, type AuditOutcome } from '@/lib/api';
import { formatDate } from '@/lib/format';

const ACTORS: Record<AuditActor, ToneLabel> = {
  agent: { label: 'Agent', tone: 'accent' },
  user: { label: 'Utilisateur', tone: 'neutral' },
  system: { label: 'Système', tone: 'neutral' },
};

const OUTCOMES: Record<AuditOutcome, ToneLabel> = {
  success: { label: 'Succès', tone: 'ok' },
  denied: { label: 'Refusé', tone: 'danger' },
  failure: { label: 'Échec', tone: 'warn' },
};

export function AuditTab() {
  const state = useApiData<AuditLog>('/admin/audit?limit=200');
  return (
    <Loaded state={state}>
      {(data) => (
        <DataTable columns={['Date', 'Acteur', 'Action', 'Résultat', 'Détail']}>
          {data.entries.map((e) => (
            <tr key={e.id}>
              <Cell className="text-xs whitespace-nowrap text-muted">
                {formatDate(e.occurredAt)}
              </Cell>
              <Cell>
                <LabelBadge labels={ACTORS} value={e.actorType} />
              </Cell>
              <Cell className="font-mono text-xs">{e.action}</Cell>
              <Cell>
                <LabelBadge labels={OUTCOMES} value={e.outcome} />
              </Cell>
              <Cell className="text-xs text-muted">
                {[e.resourceType, e.resourceId].filter(Boolean).join(' · ')}
                {e.reason && <span className="block">{e.reason}</span>}
              </Cell>
            </tr>
          ))}
          {data.entries.length === 0 && (
            <tr>
              <Cell colSpan={5} className="py-4 text-center text-muted">
                Aucune entrée.
              </Cell>
            </tr>
          )}
        </DataTable>
      )}
    </Loaded>
  );
}
