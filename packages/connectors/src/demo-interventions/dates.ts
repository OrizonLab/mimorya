/** Date helpers (UTC, ISO strings) used by the demo dataset and planning. */

/** Monday (UTC) of the week containing `date`. */
export function mondayOf(date: Date): Date {
  const d = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()));
  const day = d.getUTCDay(); // 0 = Sunday
  d.setUTCDate(d.getUTCDate() - ((day + 6) % 7));
  return d;
}

export function isoDate(date: Date): string {
  return date.toISOString().slice(0, 10);
}
