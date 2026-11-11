'use client';

import { useId } from 'react';
import { Field, Input } from '@/components/ui';
import type { RetentionDurationView } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { RETENTION_UNITS } from '@/lib/labels';

/** A duration in its unit, as people read it. */
export function inUnit(duration: RetentionDurationView, value: number): string {
  const unit = RETENTION_UNITS[duration.unit];
  return `${value} ${value > 1 ? unit.many : unit.one}`;
}

/** One kind's duration: its field within its bounds, whose it is, and,
 * while a shortening waits, when the purge applies it and what it keeps
 * meanwhile, each said to who reaches the field. */
export function DurationField({
  duration,
  value,
  onChange,
}: {
  duration: RetentionDurationView;
  value: string;
  onChange: (value: string) => void;
}) {
  const help = useId();
  const [whose, bounds, waiting] = [`${help}-whose`, `${help}-bounds`, `${help}-waiting`];
  const said = duration.shorteningAppliesAt ? [whose, bounds, waiting] : [whose, bounds];
  return (
    <div className="flex flex-col gap-1">
      <Field label={`${duration.label} (${RETENTION_UNITS[duration.unit].many})`}>
        <Input
          type="number"
          required
          min={duration.min}
          max={duration.max}
          step={1}
          aria-describedby={said.join(' ')}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
      </Field>
      <p id={whose} className="text-xs text-muted">
        {duration.own
          ? 'Durée choisie par votre entreprise.'
          : 'Durée de la plateforme, faute d’un choix de votre entreprise.'}
      </p>
      <p id={bounds} className="text-xs text-muted">
        De {duration.min} à {inUnit(duration, duration.max)} ; plateforme :{' '}
        {inUnit(duration, duration.platform)}.
      </p>
      {duration.shorteningAppliesAt && (
        <p id={waiting} className="text-xs text-muted">
          Raccourcie à {inUnit(duration, duration.duration)} : la purge l’applique à partir du{' '}
          {formatDate(duration.shorteningAppliesAt)}, et garde d’ici là{' '}
          {inUnit(duration, duration.inEffect)}.
        </p>
      )}
    </div>
  );
}
