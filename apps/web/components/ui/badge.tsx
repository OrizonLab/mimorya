import type { ReactNode } from 'react';
import { cx } from './cx';

export type Tone = 'neutral' | 'accent' | 'ok' | 'warn' | 'danger';

/** A label and the tone it is displayed with. */
export interface ToneLabel {
  label: string;
  tone: Tone;
}

const TONES: Record<Tone, string> = {
  neutral: 'bg-surface-2 text-muted',
  accent: 'bg-accent-soft text-accent',
  ok: 'bg-ok-soft text-ok',
  warn: 'bg-warn-soft text-warn',
  danger: 'bg-danger-soft text-danger',
};

export function Badge({ tone = 'neutral', children }: { tone?: Tone; children: ReactNode }) {
  return (
    <span
      className={cx(
        'inline-flex items-center gap-1 rounded-full px-2 py-0.5 text-xs font-medium whitespace-nowrap',
        TONES[tone],
      )}
    >
      {children}
    </span>
  );
}

/** Badge for a value looked up in a label table; unknown values are shown as is. */
export function LabelBadge({
  labels,
  value,
}: {
  labels: Record<string, ToneLabel>;
  value: string;
}) {
  const entry = labels[value];
  return <Badge tone={entry?.tone ?? 'neutral'}>{entry?.label ?? value}</Badge>;
}
