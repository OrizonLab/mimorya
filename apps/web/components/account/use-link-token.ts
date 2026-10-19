'use client';

import { useEffect, useState } from 'react';
import { api, ApiError, type LinkToken } from '@/lib/api';

/** A link opened before sign-in, once checked by the API. */
export type LinkState<T> =
  | { kind: 'loading' }
  | { kind: 'invalid'; message: string }
  | { kind: 'ready'; token: string; view: T };

const INCOMPLETE = 'Ce lien est incomplet : ouvrez-le tel que vous l’avez reçu.';

/**
 * Reads the token of a one-time link (`…#<token>`) and asks `path` what it
 * opens. The token travels in the fragment: never sent to a server with the
 * page. A link pasted while the page is open only changes the fragment.
 */
export function useLinkToken<T>(path: string, unchecked: string): LinkState<T> {
  const [state, setState] = useState<LinkState<T>>({ kind: 'loading' });

  useEffect(() => {
    function load() {
      const token = window.location.hash.slice(1);
      if (!token) {
        setState({ kind: 'invalid', message: INCOMPLETE });
        return;
      }
      setState({ kind: 'loading' });
      api
        .post<T>(path, { token } satisfies LinkToken)
        .then((view) => setState({ kind: 'ready', token, view }))
        .catch((e: unknown) =>
          setState({ kind: 'invalid', message: e instanceof ApiError ? e.message : unchecked }),
        );
    }
    load();
    window.addEventListener('hashchange', load);
    return () => window.removeEventListener('hashchange', load);
  }, [path, unchecked]);

  return state;
}
