import { Badge, Card, ErrorNote, LabelBadge } from '@/components/ui';
import type { RunView } from '@/lib/api';
import { formatCost, formatDate, formatNumber } from '@/lib/format';
import { AUTONOMY_LABELS, RUN_STATUS } from '@/lib/labels';
import { Stat } from './stat';

/** Objective, status, autonomy, usage and verification of one run. */
export function RunSummary({ run }: { run: RunView }) {
  return (
    <Card className="mb-6 p-4">
      <p className="text-sm">{run.objective}</p>
      <dl className="mt-4 grid grid-cols-2 gap-3 text-sm sm:grid-cols-4">
        <Stat label="Statut" value={<LabelBadge labels={RUN_STATUS} value={run.status} />} />
        <Stat label="Autonomie" value={AUTONOMY_LABELS[run.autonomyLevel] ?? run.autonomyLevel} />
        <Stat label="Étapes / outils" value={`${run.usage.steps} / ${run.usage.toolCalls}`} />
        <Stat label="Coût estimé" value={formatCost(run.usage.costMicros)} />
        <Stat
          label="Tokens (entrée / sortie)"
          value={`${formatNumber(run.usage.inputTokens)} / ${formatNumber(run.usage.outputTokens)}`}
        />
        <Stat label="Début" value={formatDate(run.createdAt)} />
        <Stat label="Fin" value={formatDate(run.finishedAt)} />
        <Stat
          label="Vérification"
          value={
            run.verification ? (
              <Badge tone={run.verification.passed ? 'ok' : 'warn'}>
                {run.verification.passed ? 'Réussie' : 'Points non vérifiés'}
              </Badge>
            ) : (
              '—'
            )
          }
        />
      </dl>
      {run.error && (
        <div className="mt-4">
          <ErrorNote message={run.error} />
        </div>
      )}
    </Card>
  );
}
