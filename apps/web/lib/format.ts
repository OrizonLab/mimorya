/** Display formats (French locale). */

/** A dollar in the millionths of a dollar costs are stored in. */
const MICROS_PER_DOLLAR = 1_000_000;

/** Estimated cost, stored in millionths of a dollar. */
export function formatCost(micros: number): string {
  return `${(micros / MICROS_PER_DOLLAR).toLocaleString('fr-FR', { style: 'currency', currency: 'USD', minimumFractionDigits: 3 })}`;
}

/** A cost in dollars, as a form's field writes it (`2.5`). */
export function dollarsOf(micros: number): string {
  return String(micros / MICROS_PER_DOLLAR);
}

/** The millionths of a dollar a field's dollars stand for (`2.50` and `2.5` alike). */
export function microsOf(dollars: string): number {
  return Math.round(Number(dollars) * MICROS_PER_DOLLAR);
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
