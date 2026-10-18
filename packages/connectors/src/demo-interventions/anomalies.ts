/** Deterministic consistency checks on intervention reports. */
import type { Intervention } from './dataset';

/** Inconsistencies detectable without interpretation (the model looks for the rest). */
export function detectAnomalies(i: Intervention): string[] {
  const anomalies: string[] = [];
  if (i.status === 'completed' && i.partsPending) {
    anomalies.push(`Statut « terminée » alors qu’une pièce est en attente (${i.partsPending})`);
  }
  if (i.status === 'completed' && !i.clientSignature) {
    anomalies.push('Intervention terminée sans signature du client');
  }
  if ((i.status === 'completed' || i.status === 'in_progress') && i.report.trim() === '') {
    anomalies.push('Compte rendu vide');
  }
  return anomalies;
}
