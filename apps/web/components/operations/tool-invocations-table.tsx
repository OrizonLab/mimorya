import { Cell, DataTable, LabelBadge } from '@/components/ui';
import type { ToolInvocation } from '@/lib/api';
import { formatTime } from '@/lib/format';
import { TOOL_STATUS } from '@/lib/labels';

/** Every tool call of a run, with the policy decision or its result. */
export function ToolInvocationsTable({ tools }: { tools: ToolInvocation[] }) {
  return (
    <DataTable columns={['Heure', 'Outil', 'Résultat', 'Détail']} className="mb-6">
      {tools.map((t) => (
        <tr key={t.id}>
          <Cell className="whitespace-nowrap text-muted">{formatTime(t.createdAt)}</Cell>
          <Cell className="font-mono text-xs">{t.toolName}</Cell>
          <Cell>
            <LabelBadge labels={TOOL_STATUS} value={t.status} />
          </Cell>
          <Cell className="text-muted">
            {t.outputSummary ?? t.decisionReason ?? '—'}
            {t.durationMs !== null && <span className="text-xs"> · {t.durationMs} ms</span>}
          </Cell>
        </tr>
      ))}
      {tools.length === 0 && (
        <tr>
          <Cell colSpan={4} className="py-4 text-center text-muted">
            Aucun outil utilisé.
          </Cell>
        </tr>
      )}
    </DataTable>
  );
}
