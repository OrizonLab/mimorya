import type { RunUsage } from '@/lib/api';

/** What a run's calls cost the company (ADR-212): their estimated cost on
 * its own key, the amount the operator re-invoices on its key; a run
 * recorded before, its cost. */
export function chargedOf(usage: RunUsage): number {
  return usage.chargedMicros ?? usage.costMicros;
}
