/** French labels of the statuses and levels shown in the interface. */
import type { ToneLabel } from '@/components/ui';

export const RISK_LABELS: Record<string, ToneLabel> = {
  read: { label: 'Lecture', tone: 'neutral' },
  draft: { label: 'Brouillon', tone: 'accent' },
  write: { label: 'Modification', tone: 'warn' },
  external: { label: 'Impact externe', tone: 'danger' },
  irreversible: { label: 'Irréversible', tone: 'danger' },
};

export const RUN_STATUS: Record<string, ToneLabel> = {
  queued: { label: 'En file', tone: 'neutral' },
  running: { label: 'En cours', tone: 'accent' },
  awaiting_approval: { label: 'Validation requise', tone: 'warn' },
  completed: { label: 'Terminée', tone: 'ok' },
  failed: { label: 'Échec', tone: 'danger' },
  cancelled: { label: 'Annulée', tone: 'neutral' },
};

export const APPROVAL_STATUS: Record<string, ToneLabel> = {
  pending: { label: 'En attente', tone: 'warn' },
  approved: { label: 'Validée', tone: 'ok' },
  rejected: { label: 'Refusée', tone: 'danger' },
  expired: { label: 'Expirée', tone: 'neutral' },
  cancelled: { label: 'Annulée', tone: 'neutral' },
};

export const TOOL_STATUS: Record<string, ToneLabel> = {
  succeeded: { label: 'Réussi', tone: 'ok' },
  failed: { label: 'Échec', tone: 'danger' },
  denied: { label: 'Refusé', tone: 'danger' },
  awaiting_approval: { label: 'Validation demandée', tone: 'warn' },
  rejected: { label: 'Rejeté', tone: 'neutral' },
};

export const AUTONOMY_LABELS: Record<string, string> = {
  read_only: 'Lecture seule',
  propose: 'Proposition',
  execute_with_approval: 'Exécution après validation',
  autonomous_limited: 'Autonomie limitée',
};
