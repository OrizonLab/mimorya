/** Working hours, busy slots and calendar helpers for the demo planning. */
import { isoDate } from './dates';
import type { DemoInterventionsStore } from './store';

/** Working day in minutes since midnight (08:00 – 17:00). */
export const WORK_START = 8 * 60;
export const WORK_END = 17 * 60;

export function toMinutes(time: string): number {
  const [h, m] = time.split(':').map(Number);
  return (h ?? 0) * 60 + (m ?? 0);
}

export function toTime(minutes: number): string {
  return `${String(Math.floor(minutes / 60)).padStart(2, '0')}:${String(minutes % 60).padStart(2, '0')}`;
}

export interface Busy {
  start: number;
  end: number;
  label: string;
}

export function busySlots(
  store: DemoInterventionsStore,
  technicianId: string,
  date: string,
): Busy[] {
  const fromInterventions = store.interventions
    .filter((i) => i.technicianId === technicianId && i.date === date && i.status !== 'cancelled')
    .map((i) => ({
      start: toMinutes(i.startTime),
      end: toMinutes(i.startTime) + i.durationHours * 60,
      label: `${i.id} ${i.type}`,
    }));
  const fromAbsences = store.absences
    .filter((a) => a.technicianId === technicianId && a.date === date)
    .map((a) => ({ start: toMinutes(a.startTime), end: toMinutes(a.endTime), label: a.reason }));
  const fromApplied = store.applied
    .filter((s) => s.technicianId === technicianId && s.date === date)
    .map((s) => ({
      start: toMinutes(s.startTime),
      end: toMinutes(s.startTime) + s.durationHours * 60,
      label: s.label,
    }));
  return [...fromInterventions, ...fromAbsences, ...fromApplied].sort((a, b) => a.start - b.start);
}

export function isWeekday(date: string): boolean {
  const day = new Date(`${date}T00:00:00Z`).getUTCDay();
  return day >= 1 && day <= 5;
}

export function datesBetween(from: string, to: string): string[] {
  const dates: string[] = [];
  const cursor = new Date(`${from}T00:00:00Z`);
  const end = new Date(`${to}T00:00:00Z`);
  while (cursor <= end && dates.length < 31) {
    dates.push(isoDate(cursor));
    cursor.setUTCDate(cursor.getUTCDate() + 1);
  }
  return dates;
}
