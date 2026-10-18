'use client';

import { useCallback, useState } from 'react';
import { ApiError } from './client';

export interface Action {
  pending: boolean;
  error: string | null;
  /** Runs `action`; returns its result, or `null` after reporting the error. */
  run: <T>(action: () => Promise<T>) => Promise<T | null>;
}

/** State of a user-triggered request: pending flag and French error message. */
export function useAction(): Action {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const run = useCallback(async <T>(action: () => Promise<T>): Promise<T | null> => {
    setPending(true);
    setError(null);
    try {
      return await action();
    } catch (e: unknown) {
      setError(e instanceof ApiError ? e.message : 'Action impossible, réessayez.');
      return null;
    } finally {
      setPending(false);
    }
  }, []);
  return { pending, error, run };
}
