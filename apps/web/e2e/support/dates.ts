/** The day `days` from today, as a date field takes it, in the browser's (and Node's) time zone. */
export function inDays(days: number): string {
  const day = new Date(Date.now() + days * 86_400_000);
  return new Date(day.getTime() - day.getTimezoneOffset() * 60_000).toISOString().slice(0, 10);
}
