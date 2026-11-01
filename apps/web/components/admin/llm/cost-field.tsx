'use client';

import { Field, Input } from '@/components/ui';
import { dollarsOf } from '@/lib/format';

/**
 * A cost at most, in dollars, within the platform's bounds: the limits of
 * the missions (ADR-093) and of the skill evaluations (ADR-155) alike.
 */
export function CostField({
  label,
  dollars,
  onChange,
  minMicros,
  maxMicros,
}: {
  label: string;
  dollars: string;
  onChange: (dollars: string) => void;
  minMicros: number;
  maxMicros: number;
}) {
  return (
    <Field label={label}>
      <Input
        type="number"
        required
        min={dollarsOf(minMicros)}
        max={dollarsOf(maxMicros)}
        step="any"
        value={dollars}
        onChange={(e) => onChange(e.target.value)}
      />
    </Field>
  );
}
