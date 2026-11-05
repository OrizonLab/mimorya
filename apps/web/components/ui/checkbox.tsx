import { Check } from 'lucide-react';
import type { InputHTMLAttributes } from 'react';
import { cx } from './cx';
import { DISABLED_LOOK } from './field';

/**
 * A checkbox drawn by the interface rather than by the browser: locked by a
 * reload, it keeps its look for 150 ms like the other controls (ADR-042),
 * where a native one greys at once. The input stays a real checkbox (its
 * label, keyboard and form behaviour); the mark is drawn over it and dims
 * with it. No indeterminate state: no checkbox of the interface has one.
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
          // The outline contrasts with the background (WCAG 1.4.11, 3:1 at least).
          'peer size-4 appearance-none rounded border border-muted bg-surface',
          'checked:border-accent checked:bg-accent',
          'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent',
          DISABLED_LOOK,
        )}
      />
      <Check
        aria-hidden
        data-checkbox-mark
        strokeWidth={3}
        // Forced colors (Windows high contrast): the check in the text's colour.
        className="pointer-events-none absolute inset-0 m-auto hidden size-3 text-accent-fg forced-color-adjust-none peer-checked:block peer-disabled:opacity-60 forced-colors:text-[CanvasText]"
      />
    </span>
  );
}
