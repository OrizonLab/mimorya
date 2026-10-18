'use client';

import { useRouter } from 'next/navigation';
import { useCallback, useEffect, useState } from 'react';
import { api, ApiError, type Me } from '@/lib/api';

const PENDING_REFRESH_MS = 20_000;

/** Signed-in user; redirects to the login page when the session is missing. */
export function useCurrentUser(): { me: Me | null; logout: () => Promise<void> } {
  const router = useRouter();
  const [me, setMe] = useState<Me | null>(null);

  useEffect(() => {
    api
      .get<Me>('/auth/me')
      .then(setMe)
      .catch((error: unknown) => {
        if (error instanceof ApiError && error.status === 401) router.replace('/login');
      });
  }, [router]);

  const logout = useCallback(async () => {
    await api.post('/auth/logout').catch(() => undefined);
    router.replace('/login');
  }, [router]);

  return { me, logout };
}

/** Number of approvals awaiting the user, refreshed periodically and on navigation. */
export function usePendingApprovals(enabled: boolean, pathname: string): number {
  const [pending, setPending] = useState(0);

  const refresh = useCallback(() => {
    api
      .get<{ approvals: unknown[] }>('/approvals')
      .then((r) => setPending(r.approvals.length))
      .catch(() => undefined);
  }, []);

  useEffect(() => {
    if (!enabled) return;
    refresh();
    const timer = setInterval(refresh, PENDING_REFRESH_MS);
    return () => clearInterval(timer);
  }, [enabled, refresh, pathname]);

  return pending;
}
