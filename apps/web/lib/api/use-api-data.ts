'use client';

import { useCallback, useEffect, useState } from 'react';
import { ApiError, api } from './client';

export interface ApiData<T> {
  data: T | null;
  error: string | null;
  /** A request is under way: `data`, if any, is the former one. */
  loading: boolean;
  reload: () => void;
}

/** Loads `path` on mount and whenever it changes; errors are reported, not swallowed. */
export function useApiData<T>(path: string): ApiData<T> {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [version, setVersion] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    setLoading(true);
    api
      .get<T>(path)
      .then((value) => !cancelled && setData(value))
      .catch(
        (e: unknown) =>
          !cancelled && setError(e instanceof ApiError ? e.message : 'Chargement impossible'),
      )
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [path, version]);

  const reload = useCallback(() => setVersion((v) => v + 1), []);
  return { data, error, loading, reload };
}
