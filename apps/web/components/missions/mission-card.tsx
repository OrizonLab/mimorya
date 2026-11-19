'use client';

import { AlertTriangle, ShieldCheck } from 'lucide-react';
import Link from 'next/link';
import { Badge, Button, Card, ErrorNote, LabelBadge, Markdown } from '@/components/ui';
import { api, useAction, type Mission, type Verification } from '@/lib/api';
import { formatCost, formatDate } from '@/lib/format';
import { MISSION_STATUS } from '@/lib/labels';

/** A mission still under way: its stage runs or waits for a decision. */
export function underWay(mission: Mission): boolean {
  return mission.status === 'in_progress' || mission.status === 'awaiting_approval';
}

/**
 * One of the reader's missions (ADR-089): how it stands, how far it went
 * and what it cost against its limits, what it was entrusted with, and how
 * it ended. Its stages are followed in its conversation; one under way can
 * be stopped from here.
 */
export function MissionCard({ mission, onChanged }: { mission: Mission; onChanged: () => void }) {
  const action = useAction();

  async function stop() {
    if (!window.confirm(`Arrêter « ${mission.title} » ? L’étape en cours est abandonnée.`)) return;
    const stopped = await action.run(() => api.post(`/missions/${mission.id}/stop`));
    if (stopped !== null) onChanged();
  }

  const verification = mission.verification;
  return (
    // Named by its title: each mission is found by it.
    <article aria-label={mission.title}>
      <Card className="flex flex-col gap-2 p-4">
        <div className="flex flex-wrap items-center gap-2">
          <p className="font-medium">{mission.title}</p>
          <LabelBadge labels={MISSION_STATUS} value={mission.status} />
        </div>
        <p className="text-xs text-muted">
          Confiée le {formatDate(mission.createdAt)} · étape {mission.stages} sur{' '}
          {mission.maxStages} au plus · coût estimé {formatCost(mission.costMicros)} sur{' '}
          {formatCost(mission.maxCostMicros)} au plus
        </p>
        <p className="text-sm whitespace-pre-line">{mission.objective}</p>
        <div className="text-sm">
          <p className="text-muted">Critères de réussite :</p>
          <ul className="ml-5 list-disc">
            {mission.successCriteria.map((criterion, index) => (
              // Two criteria may read the same.
              <li key={index}>{criterion}</li>
            ))}
          </ul>
        </div>
        {mission.result && (
          <section aria-label="Résultat" className="rounded-lg border border-line p-3">
            <Markdown>{mission.result}</Markdown>
            {verification && <Review verification={verification} />}
          </section>
        )}
        {mission.error && <p className="text-sm text-danger">{mission.error}</p>}
        <div className="flex flex-wrap items-center gap-3">
          <Link className="w-fit text-sm underline" href={`/chat/${mission.conversationId}`}>
            Suivre dans la conversation
          </Link>
          {underWay(mission) && (
            <Button
              variant="danger"
              aria-label={`Arrêter « ${mission.title} »`}
              loading={action.pending}
              onClick={() => void stop()}
            >
              Arrêter
            </Button>
          )}
        </div>
        {action.error && <ErrorNote message={action.error} />}
      </Card>
    </article>
  );
}

/**
 * What the review of the final answer found. Its criteria are said to be
 * checked only when a reviewer judged them: an answer it could not judge
 * (reviewer unavailable, no usable verdict) passes, unchecked.
 */
function Review({ verification }: { verification: Verification }) {
  if (!verification.passed) {
    return (
      <Badge tone="warn">
        <AlertTriangle className="size-3" aria-hidden />
        Points non vérifiés
      </Badge>
    );
  }
  if (!verification.judged) {
    return (
      <Badge tone="warn">
        <AlertTriangle className="size-3" aria-hidden />
        Critères non contrôlés (relecture indisponible)
      </Badge>
    );
  }
  return (
    <Badge tone="ok">
      <ShieldCheck className="size-3" aria-hidden />
      Critères vérifiés
    </Badge>
  );
}
