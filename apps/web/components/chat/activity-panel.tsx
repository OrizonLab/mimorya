'use client';

import { Ban, Check, Hourglass, Loader2, ShieldAlert, XCircle } from 'lucide-react';
import type { RunActivity } from './use-run-stream';

export function ActivityPanel({ activity }: { activity: RunActivity }) {
  const working = activity.status === 'running';
  return (
    <div
      className="rounded-2xl rounded-bl-sm border border-line bg-surface px-4 py-3"
      aria-live="polite"
    >
      <div className="flex items-center gap-2 text-sm font-medium">
        {working ? (
          <Loader2 className="size-4 animate-spin text-accent" aria-hidden />
        ) : (
          <Hourglass className="size-4 text-warn" aria-hidden />
        )}
        {working ? 'Je travaille sur votre demande…' : 'En attente de votre validation'}
      </div>
      {(activity.notes.length > 0 || activity.steps.length > 0) && (
        <ol className="mt-3 flex flex-col gap-1.5 border-l border-line pl-3 text-sm">
          {activity.notes.map((note, i) => (
            <li key={`n${i}`} className="text-muted italic">
              {note}
            </li>
          ))}
          {activity.steps.map((step) => (
            <li key={step.toolCallId} className="flex items-start gap-2">
              <StepIcon step={step} />
              <span className="min-w-0">
                <span className="font-medium">{step.title}</span>
                {step.summary && <span className="text-muted"> — {step.summary}</span>}
                {step.decision === 'deny' && step.reason && (
                  <span className="block text-xs text-danger">Refusé : {step.reason}</span>
                )}
                {step.decision === 'require_approval' && !step.done && (
                  <span className="block text-xs text-warn">Soumis à votre validation</span>
                )}
              </span>
            </li>
          ))}
        </ol>
      )}
      {activity.verification && !activity.verification.passed && working && (
        <p className="mt-3 text-xs text-warn">
          Contrôle qualité : correction en cours ({activity.verification.issues.length} point(s)).
        </p>
      )}
      {activity.draft && (
        <p className="mt-3 line-clamp-6 text-sm whitespace-pre-wrap text-muted">{activity.draft}</p>
      )}
    </div>
  );
}

function StepIcon({ step }: { step: RunActivity['steps'][number] }) {
  const cls = 'mt-0.5 size-4 shrink-0';
  if (step.decision === 'deny') return <Ban className={`${cls} text-danger`} aria-label="Refusé" />;
  if (step.decision === 'require_approval' && !step.done) {
    return <ShieldAlert className={`${cls} text-warn`} aria-label="Validation requise" />;
  }
  if (!step.done)
    return <Loader2 className={`${cls} animate-spin text-accent`} aria-label="En cours" />;
  if (step.isError) return <XCircle className={`${cls} text-danger`} aria-label="Erreur" />;
  return <Check className={`${cls} text-ok`} aria-label="Terminé" />;
}
