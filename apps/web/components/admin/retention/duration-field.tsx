'use client';

import { Field, Input } from '@/components/ui';
import type { RetentionDurationView } from '@/lib/api';
import { formatDate } from '@/lib/format';

/** A duration in its unit, as people read it. */
export function inUnit(duration: RetentionDurationView, value: number): string {
  return `${value} ${duration.unit === 'months' ? 'mois' : value > 1 ? 'jours' : 'jour'}`;
}

/** One kind's duration: its field within its bounds, what the purge applies
 * now and, while a shortening waits, when it applies. */
export function DurationField({
  duration,
  value,
  onChange,
}: {
  duration: RetentionDurationView;
  value: string;
  onChange: (value: string) => void;
}) {
  const unit = duration.unit === 'months' ? 'mois' : 'jours';
  return (
    <div className="flex flex-col gap-1">
      <Field label={`${duration.label} (${unit})`}>
        <Input
          type="number"
          required
          min={duration.min}
          max={duration.max}
          step={1}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      </Field>
      <p className="text-xs text-muted">
        De {duration.min} à {inUnit(duration, duration.max)} ; plateforme :{' '}
        {inUnit(duration, duration.platform)}.
      </p>
      {duration.shorteningAppliesAt && (
        <p className="text-xs text-muted">
          Raccourcie à {inUnit(duration, duration.duration)} : la purge l’applique à partir du{' '}
          {formatDate(duration.shorteningAppliesAt)}, et garde d’ici là{' '}
          {inUnit(duration, duration.inEffect)}.
        </p>
      )}
    </div>
  );
}
