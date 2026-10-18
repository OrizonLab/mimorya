'use client';

import {
  Activity,
  BookOpen,
  CheckCircle2,
  LogOut,
  Menu,
  MessageSquare,
  Settings,
  X,
} from 'lucide-react';
import Link from 'next/link';
import { usePathname, useRouter } from 'next/navigation';
import { useCallback, useEffect, useState, type ReactNode } from 'react';
import { api, ApiError, type Me } from '@/lib/api';
import { MeContext } from '@/lib/me-context';
import { Badge, cx, Spinner } from './ui';

interface NavItem {
  href: string;
  label: string;
  icon: typeof MessageSquare;
  visible?: (me: Me) => boolean;
}

const ADMIN_PERMISSIONS = [
  'org:read',
  'user:read',
  'role:manage',
  'audit:read',
  'usage:read',
  'connector:manage',
];

const NAV: NavItem[] = [
  { href: '/chat', label: 'Conversations', icon: MessageSquare },
  { href: '/approvals', label: 'Validations', icon: CheckCircle2 },
  { href: '/operations', label: 'Opérations', icon: Activity },
  { href: '/memory', label: 'Mémoire', icon: BookOpen },
  {
    href: '/admin',
    label: 'Administration',
    icon: Settings,
    visible: (me) => me.permissions.some((p) => ADMIN_PERMISSIONS.includes(p)),
  },
];

export function AppShell({ children }: { children: ReactNode }) {
  const router = useRouter();
  const pathname = usePathname();
  const [me, setMe] = useState<Me | null>(null);
  const [pending, setPending] = useState(0);
  const [menuOpen, setMenuOpen] = useState(false);

  useEffect(() => {
    api
      .get<Me>('/auth/me')
      .then(setMe)
      .catch((error: unknown) => {
        if (error instanceof ApiError && error.status === 401) router.replace('/login');
      });
  }, [router]);

  const refreshPending = useCallback(() => {
    api
      .get<{ approvals: unknown[] }>('/approvals')
      .then((r) => setPending(r.approvals.length))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!me) return;
    refreshPending();
    const timer = setInterval(refreshPending, 20_000);
    return () => clearInterval(timer);
  }, [me, refreshPending, pathname]);

  useEffect(() => setMenuOpen(false), [pathname]);

  async function logout() {
    await api.post('/auth/logout').catch(() => undefined);
    router.replace('/login');
  }

  if (!me) {
    return (
      <div className="flex min-h-dvh items-center justify-center">
        <Spinner />
      </div>
    );
  }

  const nav = NAV.filter((item) => !item.visible || item.visible(me));
  const sidebar = (
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
      {nav.map((item) => {
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
          onClick={logout}
          className="mt-3 flex items-center gap-2 text-xs text-muted hover:text-fg"
        >
          <LogOut className="size-3.5" aria-hidden /> Se déconnecter
        </button>
      </div>
    </nav>
  );

  return (
    <MeContext.Provider value={me}>
      <div className="flex min-h-dvh">
        <aside className="sticky top-0 hidden h-dvh w-60 shrink-0 border-r border-line bg-surface lg:block">
          {sidebar}
        </aside>
        {menuOpen && (
          <div className="fixed inset-0 z-40 lg:hidden">
            <button
              type="button"
              aria-label="Fermer le menu"
              className="absolute inset-0 bg-black/30"
              onClick={() => setMenuOpen(false)}
            />
            <aside className="absolute inset-y-0 left-0 w-64 border-r border-line bg-surface">
              <button
                type="button"
                aria-label="Fermer le menu"
                className="absolute top-3 right-3 text-muted"
                onClick={() => setMenuOpen(false)}
              >
                <X className="size-5" />
              </button>
              {sidebar}
            </aside>
          </div>
        )}
        <div className="flex min-w-0 flex-1 flex-col">
          <header className="sticky top-0 z-30 flex items-center gap-3 border-b border-line bg-surface px-4 py-3 lg:hidden">
            <button type="button" aria-label="Ouvrir le menu" onClick={() => setMenuOpen(true)}>
              <Menu className="size-5" />
            </button>
            <span className="font-semibold">{me.assistant.name}</span>
            {pending > 0 && (
              <Link href="/approvals" className="ml-auto">
                <Badge tone="warn">{pending} validation(s)</Badge>
              </Link>
            )}
          </header>
          <main className="flex min-w-0 flex-1 flex-col">{children}</main>
        </div>
      </div>
    </MeContext.Provider>
  );
}
