'use client';

import { useEffect } from 'react';

/**
 * Calls `refresh` every `everyMs` while `active`: what a list shows of work
 * under way elsewhere (an evaluation played by the server, ADR-104) is
 * read again until it ends.
 */
export function useRefreshWhile(active: boolean, refresh: () => void, everyMs: number) {
  useEffect(() => {
    if (!active) return;
    const timer = setInterval(refresh, everyMs);
    return () => clearInterval(timer);
  }, [active, refresh, everyMs]);
}
