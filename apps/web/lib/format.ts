/** Display formats (French locale). */

/** Estimated cost, stored in millionths of a dollar. */
export function formatCost(micros: number): string {
  return `${(micros / 1_000_000).toLocaleString('fr-FR', { style: 'currency', currency: 'USD', minimumFractionDigits: 3 })}`;
}

/** Date and time, in `timeZone` when given (a scheduled task's), else the reader's. */
export function formatDate(value: string | null | undefined, timeZone?: string): string {
  if (!value) return '—';
  return new Date(value).toLocaleString('fr-FR', {
    dateStyle: 'short',
    timeStyle: 'short',
    timeZone,
  });
}

export function formatNumber(value: number): string {
  return value.toLocaleString('fr-FR');
}

export function formatTime(value: string): string {
  return new Date(value).toLocaleTimeString('fr-FR');
}

/** A size in bytes, as people read it: « 820 o », « 12,4 Ko », « 3,1 Mo ». */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} o`;
  const kilo = bytes / 1024;
  if (kilo < 1024) return `${formatNumber(Math.round(kilo * 10) / 10)} Ko`;
  return `${formatNumber(Math.round((kilo / 1024) * 10) / 10)} Mo`;
}
