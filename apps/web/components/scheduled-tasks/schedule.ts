/** When a scheduled task runs (ADR-080), as it is stored and as it is read. */

/** The days of the week as a task stores them: 1 is Monday (ISO). */
export const WEEKDAYS = [
  { day: 1, short: 'Lun', name: 'lundi' },
  { day: 2, short: 'Mar', name: 'mardi' },
  { day: 3, short: 'Mer', name: 'mercredi' },
  { day: 4, short: 'Jeu', name: 'jeudi' },
  { day: 5, short: 'Ven', name: 'vendredi' },
  { day: 6, short: 'Sam', name: 'samedi' },
  { day: 7, short: 'Dim', name: 'dimanche' },
] as const;

/** « a, b et c ». */
function joined(words: string[]): string {
  return words.length < 2 ? words.join('') : `${words.slice(0, -1).join(', ')} et ${words.at(-1)}`;
}

/** `HH:MM` as it is said: « 8 h 05 ». */
export function spokenTime(time: string): string {
  const [hours, minutes] = time.split(':');
  return `${Number(hours)} h ${minutes}`;
}

/** When it runs, in words: « Du lundi au vendredi à 8 h 00 ». */
export function describeSchedule(days: number[], time: string): string {
  const sorted = [...new Set(days)].sort((a, b) => a - b);
  const names = WEEKDAYS.filter((d) => sorted.includes(d.day)).map((d) => d.name);
  const when =
    sorted.length === 7
      ? 'Tous les jours'
      : sorted.join() === '1,2,3,4,5'
        ? 'Du lundi au vendredi'
        : `Chaque ${joined(names)}`;
  return `${when} à ${spokenTime(time)}`;
}

/** The reader's time zone, as the browser knows it. */
export function readerTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

/** The time zones one may choose, `current` among them. */
export function timeZones(current: string): string[] {
  const known = Intl.supportedValuesOf('timeZone');
  return known.includes(current) ? known : [current, ...known];
}
