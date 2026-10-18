/** Display formats (French locale). */

/** Estimated cost, stored in millionths of a dollar. */
export function formatCost(micros: number): string {
  return `${(micros / 1_000_000).toLocaleString('fr-FR', { style: 'currency', currency: 'USD', minimumFractionDigits: 3 })}`;
}

export function formatDate(value: string | null | undefined): string {
  if (!value) return '—';
  return new Date(value).toLocaleString('fr-FR', { dateStyle: 'short', timeStyle: 'short' });
}

export function formatNumber(value: number): string {
  return value.toLocaleString('fr-FR');
}

export function formatTime(value: string): string {
  return new Date(value).toLocaleTimeString('fr-FR');
}
