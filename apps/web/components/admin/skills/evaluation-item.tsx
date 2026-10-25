'use client';

import { useState } from 'react';
import { Badge, Button, LabelBadge } from '@/components/ui';
import type { SkillEvaluationView } from '@/lib/api';
import { formatCost, formatDate } from '@/lib/format';
import { EVALUATION_STATUS } from '@/lib/labels';
import { EvaluationDetail } from './evaluation-detail';

/**
 * One evaluation of a skill (ADR-104): its state, what its cases add up to
 * for the version evaluated and for the active one beside it, the
 * regressions found; its cases read on demand.
 */
export function EvaluationItem({ evaluation }: { evaluation: SkillEvaluationView }) {
  const [open, setOpen] = useState(false);
  const e = evaluation;
  const ended = e.status === 'completed' || e.status === 'failed';
  return (
    <li className="rounded-lg bg-surface-2 p-3 text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <span className="font-medium">Évaluation de la version {e.version}</span>
        <LabelBadge labels={EVALUATION_STATUS} value={e.status} />
        {e.regressions > 0 && (
          <Badge tone="danger">
            {e.regressions} régression{e.regressions > 1 ? 's' : ''}
          </Badge>
        )}
        <Button
          variant="ghost"
          className="ml-auto px-2 py-1"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
        >
          {open ? 'Masquer les cas' : 'Voir les cas'}
          <span className="sr-only"> de l’évaluation de la version {e.version}</span>
        </Button>
      </div>
      <p className="mt-1 text-xs">
        {ended
          ? `Version ${e.version} : ${e.passed} cas réussi${e.passed > 1 ? 's' : ''} sur ${e.cases}`
          : `${e.played} cas joué${e.played > 1 ? 's' : ''} sur ${e.cases}`}
        {e.referenceVersion !== null &&
          e.referencePassed !== null &&
          ` · version active ${e.referenceVersion} : ${e.referencePassed} sur ${e.cases}`}
      </p>
      {e.failure && <p className="mt-1 text-xs text-danger">{e.failure}</p>}
      <p className="mt-1 text-xs text-muted">
        Demandée par {e.launchedBy ?? 'un compte supprimé'} le {formatDate(e.createdAt)}
        {ended && ` · coût ${formatCost(e.costMicros)}`}
      </p>
      {/* Read again as the evaluation goes on. */}
      {open && <EvaluationDetail key={`${e.status} ${e.played}`} id={e.id} />}
    </li>
  );
}
