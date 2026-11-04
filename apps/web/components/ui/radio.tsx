import type { InputHTMLAttributes } from 'react';
import { cx } from './cx';
import { DISABLED_LOOK } from './field';

/**
 * A radio button drawn by the interface rather than by the browser, like
 * `Checkbox` (ADR-045): locked by a reload, it keeps its look for 150 ms
 * like the other controls (ADR-042), where a native one greys at once. The
 * input stays a real radio (its label, its group, keyboard and form
 * behaviour); the dot is drawn over it and dims with it.
 */
export function Radio({
  className,
  ...props
}: Omit<InputHTMLAttributes<HTMLInputElement>, 'type'>) {
  return (
    <span className={cx('relative inline-flex size-4 shrink-0', className)}>
      <input
        type="radio"
        {...props}
        className={cx(
          // The outline contrasts with the background (WCAG 1.4.11, 3:1 at least).
          'peer size-4 appearance-none rounded-full border border-muted bg-surface',
          'checked:border-accent',
          'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent',
          DISABLED_LOOK,
        )}
      />
      <span
        aria-hidden
        data-checkbox-mark
        className="pointer-events-none absolute inset-0 m-auto hidden size-2 rounded-full bg-accent peer-checked:block peer-disabled:opacity-60"
      />
    </span>
  );
}
