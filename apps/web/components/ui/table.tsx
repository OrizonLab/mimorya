/** Data table inside a card: column headers, then the caller's rows. */
import type { ReactNode, TdHTMLAttributes } from 'react';
import { Card } from './card';
import { cx } from './cx';

export function DataTable({
  columns,
  className,
  children,
}: {
  columns: string[];
  className?: string;
  children: ReactNode;
}) {
  return (
    <Card className={cx('overflow-x-auto', className)}>
      <table className="w-full text-sm">
        <thead className="bg-surface-2 text-left text-xs text-muted">
          <tr>
            {columns.map((c) => (
              <th key={c} className="px-4 py-2 font-medium">
                {c}
              </th>
            ))}
          </tr>
        </thead>
        <tbody className="divide-y divide-line">{children}</tbody>
      </table>
    </Card>
  );
}

export function Cell({ className, ...props }: TdHTMLAttributes<HTMLTableCellElement>) {
  return <td {...props} className={cx('px-4 py-2', className)} />;
}
