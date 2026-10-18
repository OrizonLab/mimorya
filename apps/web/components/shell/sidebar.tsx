import { LogOut } from 'lucide-react';
import Link from 'next/link';
import { Badge, cx } from '@/components/ui';
import type { Me } from '@/lib/api';
import { NAV_ITEMS } from './nav';

export function Sidebar({
  me,
  pathname,
  pending,
  onLogout,
}: {
  me: Me;
  pathname: string;
  pending: number;
  onLogout: () => void;
}) {
  const items = NAV_ITEMS.filter((item) => !item.visible || item.visible(me));
  return (
    <nav className="flex h-full flex-col gap-1 p-3" aria-label="Navigation principale">
      <div className="mb-4 px-2 pt-1">
        <p className="text-base font-semibold tracking-tight">{me.assistant.name}</p>
        <p className="truncate text-xs text-muted">{me.tenant.name}</p>
        {me.tenant.demo && (
          <div className="mt-2">
            <Badge tone="warn">Données fictives</Badge>
          </div>
        )}
      </div>
      {items.map((item) => {
        const active = pathname === item.href || pathname.startsWith(`${item.href}/`);
        const Icon = item.icon;
        return (
          <Link
            key={item.href}
            href={item.href}
            aria-current={active ? 'page' : undefined}
            className={cx(
              'flex items-center gap-3 rounded-lg px-3 py-2 text-sm transition',
              active
                ? 'bg-accent-soft font-medium text-accent'
                : 'text-muted hover:bg-surface-2 hover:text-fg',
            )}
          >
            <Icon className="size-4" aria-hidden />
            <span className="flex-1">{item.label}</span>
            {item.href === '/approvals' && pending > 0 && <Badge tone="warn">{pending}</Badge>}
          </Link>
        );
      })}
      <div className="mt-auto border-t border-line px-2 pt-3">
        <p className="truncate text-sm font-medium">{me.user.displayName}</p>
        <p className="truncate text-xs text-muted">{me.user.jobTitle}</p>
        <button
          type="button"
          onClick={onLogout}
          className="mt-3 flex items-center gap-2 text-xs text-muted hover:text-fg"
        >
          <LogOut className="size-3.5" aria-hidden /> Se déconnecter
        </button>
      </div>
    </nav>
  );
}
