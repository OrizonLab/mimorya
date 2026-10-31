/** Labelled form controls sharing one input style. */
import type {
  ComponentProps,
  ReactNode,
  SelectHTMLAttributes,
  TextareaHTMLAttributes,
} from 'react';
import { cx } from './cx';

/**
 * How a disabled control looks, for those not built with `Input` or `Select`.
 * Locked by a reload, it takes this look only past 150 ms (`globals.css`).
 */
export const DISABLED_LOOK = 'disabled:cursor-not-allowed disabled:opacity-60';

const CONTROL = `rounded-lg border border-line bg-surface px-3 py-2 text-sm text-fg outline-none focus:border-accent ${DISABLED_LOOK}`;

export function Field({
  label,
  className,
  children,
}: {
  label: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <label className={cx('flex flex-col gap-1 text-xs text-muted', className)}>
      {label}
      {children}
    </label>
  );
}

/** A text, file or other input; its `ref` reaches the element (React 19). */
export function Input({ className, ...props }: ComponentProps<'input'>) {
  return <input {...props} className={cx(CONTROL, className)} />;
}

export function Select({ className, ...props }: SelectHTMLAttributes<HTMLSelectElement>) {
  return <select {...props} className={cx(CONTROL, className)} />;
}

export function Textarea({ className, ...props }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea {...props} className={cx(CONTROL, className)} />;
}
