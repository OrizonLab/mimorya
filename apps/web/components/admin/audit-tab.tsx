'use client';

import type { ReactNode } from 'react';
import { Cell, DataTable, LabelBadge, Loaded, type ToneLabel } from '@/components/ui';
import { useApiData, type AuditActor, type AuditLog, type AuditOutcome } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { useCan } from '@/lib/me-context';
import { ChainCheck } from './audit-chain/chain-check';
import { ExportTokens } from './audit-export/export-tokens';

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

/**
 * The « Journal d’audit » tab (ADR-182): the log and its verification for
 * who reads it (`audit:read`), the SIEM's export tokens for who exports it
 * (`audit:export`); either opens the tab, each part shown to its holder.
 */
export function AuditTab() {
  const reads = useCan('audit:read');
  const exports = useCan('audit:export');
  if (!reads) {
    return exports ? <ExportTokens /> : null;
  }
  return <AuditLogView>{(reload) => exports && <ExportTokens onChanged={reload} />}</AuditLogView>;
}

/**
 * The log, its verification first, then what `between` shows, given what
 * reloads the log.
 */
function AuditLogView({ children: between }: { children: (reload: () => void) => ReactNode }) {
  const state = useApiData<AuditLog>('/admin/audit?limit=200');
  return (
    <div className="flex flex-col gap-4">
      <ChainCheck onVerified={state.reload} />
      {between(state.reload)}
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
    </div>
  );
}
