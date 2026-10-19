'use client';

import { useCallback, useEffect, useState } from 'react';
import { api, loadFailure } from './client';

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
  const [version, setVersion] = useState(0);
  // The request whose answer is shown: until it is the latest one, another is
  // under way, from the render that asked for it (a reload or another path).
  const request = `${version} ${path}`;
  const [settled, setSettled] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setError(null);
    api
      .get<T>(path)
      .then((value) => !cancelled && setData(value))
      .catch((e: unknown) => !cancelled && setError(loadFailure(e)))
      .finally(() => !cancelled && setSettled(`${version} ${path}`));
    return () => {
      cancelled = true;
    };
  }, [path, version]);

  const reload = useCallback(() => setVersion((v) => v + 1), []);
  return { data, error, loading: settled !== request, reload };
}
