import { Check } from 'lucide-react';
import type { InputHTMLAttributes } from 'react';
import { cx } from './cx';

/**
 * A checkbox drawn by the interface rather than by the browser: locked by a
 * reload, it keeps its look for 150 ms like the other controls (ADR-042),
 * where a native one greys at once. The input stays a real checkbox (its
 * label, keyboard and form behaviour); the mark is drawn over it.
 */
export function Checkbox({
  className,
  ...props
}: Omit<InputHTMLAttributes<HTMLInputElement>, 'type'>) {
  return (
    <span className={cx('relative inline-flex size-4 shrink-0', className)}>
      <input
        type="checkbox"
        {...props}
        className={cx(
          'peer size-4 appearance-none rounded border border-line bg-surface',
          'checked:border-accent checked:bg-accent',
          'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent',
          'disabled:cursor-not-allowed disabled:opacity-60',
        )}
      />
      <Check
        aria-hidden
        strokeWidth={3}
        className="pointer-events-none absolute inset-0 m-auto hidden size-3 text-accent-fg peer-checked:block"
      />
    </span>
  );
}
