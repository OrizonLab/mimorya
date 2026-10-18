'use client';

import { Menu, X } from 'lucide-react';
import Link from 'next/link';
import { usePathname } from 'next/navigation';
import { useEffect, useState, type ReactNode } from 'react';
import { FirstPasswordScreen } from '@/components/account';
import { Badge, Spinner } from '@/components/ui';
import { MeContext } from '@/lib/me-context';
import { Sidebar } from './sidebar';
import { useCurrentUser, usePendingApprovals } from './use-session';

/** Authenticated layout: navigation (sidebar, or drawer on small screens) and content. */
export function AppShell({ children }: { children: ReactNode }) {
  const pathname = usePathname();
  const { me, logout } = useCurrentUser();
  const pending = usePendingApprovals(me !== null, pathname);
  const [menuOpen, setMenuOpen] = useState(false);

  useEffect(() => setMenuOpen(false), [pathname]);

  if (!me) {
    return (
      <div className="flex min-h-dvh items-center justify-center">
        <Spinner />
      </div>
    );
  }

  if (me.user.mustChangePassword) {
    return <FirstPasswordScreen me={me} onLogout={() => void logout()} />;
  }

  const sidebar = (
    <Sidebar me={me} pathname={pathname} pending={pending} onLogout={() => void logout()} />
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
