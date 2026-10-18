import type { ReactNode } from 'react';
import { cx } from './cx';

export function Card({ className, children }: { className?: string; children: ReactNode }) {
  return (
    <div className={cx('rounded-xl border border-line bg-surface shadow-sm', className)}>
      {children}
    </div>
  );
}
