'use client';

import { Checkbox } from '@/components/ui';
import { WEEKDAYS } from './schedule';

/** The days a task runs, Monday first; each checkbox named by its day. */
export function WeekdayPicker({
  value,
  onChange,
}: {
  value: number[];
  onChange: (days: number[]) => void;
}) {
  function toggle(day: number, checked: boolean) {
    const others = value.filter((d) => d !== day);
    onChange(checked ? [...others, day].sort((a, b) => a - b) : others);
  }

  return (
    <fieldset className="flex flex-col gap-1 text-xs text-muted">
      <legend className="mb-1">Jours</legend>
      <div className="flex flex-wrap gap-3">
        {WEEKDAYS.map((d) => (
          <label key={d.day} className="flex items-center gap-1.5 text-sm text-fg">
            <Checkbox
              aria-label={d.name}
              checked={value.includes(d.day)}
              onChange={(e) => toggle(d.day, e.target.checked)}
            />
            {d.short}
          </label>
        ))}
      </div>
    </fieldset>
  );
}
