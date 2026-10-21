'use client';

import { useRouter } from 'next/navigation';
import { useCallback, useEffect, useState } from 'react';
import { api, ApiError, loadFailure, type ApprovalList, type Me } from '@/lib/api';

const PENDING_REFRESH_MS = 20_000;

/**
 * Signed-in user; redirects to the login page when the session is missing.
 * Any other failure (a timeout included) is given, with a way to retry.
 */
export function useCurrentUser(): {
  me: Me | null;
  error: string | null;
  retry: () => void;
  logout: () => Promise<void>;
} {
  const router = useRouter();
  const [me, setMe] = useState<Me | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [attempt, setAttempt] = useState(0);

  useEffect(() => {
    setError(null);
    api
      .get<Me>('/auth/me')
      .then(setMe)
      .catch((failure: unknown) => {
        if (failure instanceof ApiError && failure.status === 401) router.replace('/login');
        else setError(loadFailure(failure));
      });
  }, [router, attempt]);

  const retry = useCallback(() => setAttempt((n) => n + 1), []);

  const logout = useCallback(async () => {
    await api.post('/auth/logout').catch(() => undefined);
    router.replace('/login');
  }, [router]);

  return { me, error, retry, logout };
}

/**
 * Number of approvals the user may decide (theirs, or as a manager, ADR-059),
 * refreshed periodically and on navigation.
 */
export function usePendingApprovals(enabled: boolean, pathname: string): number {
  const [pending, setPending] = useState(0);

  const refresh = useCallback(() => {
    api
      .get<ApprovalList>('/approvals')
      .then((r) => setPending(r.approvals.filter((a) => a.canDecide).length))
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
