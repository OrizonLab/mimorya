import { Loader2 } from 'lucide-react';
import type { ComponentProps } from 'react';
import { cx } from './cx';

type Variant = 'primary' | 'secondary' | 'ghost' | 'danger';

const VARIANTS: Record<Variant, string> = {
  primary: 'bg-accent text-accent-fg hover:opacity-90',
  secondary: 'bg-surface text-fg border border-line hover:bg-surface-2',
  ghost: 'text-muted hover:text-fg hover:bg-surface-2',
  danger: 'bg-surface text-danger border border-line hover:bg-danger-soft',
};

export function Button({
  variant = 'primary',
  loading,
  className,
  children,
  ...props
}: ComponentProps<'button'> & { variant?: Variant; loading?: boolean }) {
  return (
    <button
      {...props}
      disabled={props.disabled || loading}
      className={cx(
        'inline-flex items-center justify-center gap-2 rounded-lg px-3.5 py-2 text-sm font-medium transition',
        'focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent',
        'disabled:cursor-not-allowed disabled:opacity-50',
        VARIANTS[variant],
        className,
      )}
    >
      {loading && <Loader2 className="size-4 animate-spin" aria-hidden />}
      {children}
    </button>
  );
}
