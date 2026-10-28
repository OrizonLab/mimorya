'use client';

import { useEffect, useState } from 'react';
import { api, ApiError, type LinkToken } from '@/lib/api';

/** A link opened before sign-in, or in the person's own session, once checked by the API. */
export type LinkState<T> =
  | { kind: 'loading' }
  | { kind: 'invalid'; message: string }
  // A link that only opens in the person's own session, opened signed out.
  | { kind: 'signed-out' }
  | { kind: 'ready'; token: string; view: T };

const INCOMPLETE = 'Ce lien est incomplet : ouvrez-le tel que vous l’avez reçu.';

/** The body that asks about a one-time link's token. */
const asToken = (token: string) => ({ token }) satisfies LinkToken;

/**
 * Reads the token of a link (`…#<token>`) and asks `path` what it opens,
 * with the body `ask` makes of it (a constant function: the link is asked
 * about again when it changes). The token travels in the fragment: never
 * sent to a server with the page. A link pasted while the page is open only
 * changes the fragment. A link asked about in the person's own session
 * (ADR-126) answers 401 when they are signed out: they are told to sign in
 * and open it again, the sign-in page not keeping the fragment.
 */
export function useLinkToken<T>(
  path: string,
  unchecked: string,
  ask: (token: string) => unknown = asToken,
): LinkState<T> {
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
        .post<T>(path, ask(token))
        .then((view) => setState({ kind: 'ready', token, view }))
        .catch((e: unknown) => {
          if (e instanceof ApiError && e.status === 401) setState({ kind: 'signed-out' });
          else
            setState({ kind: 'invalid', message: e instanceof ApiError ? e.message : unchecked });
        });
    }
    load();
    window.addEventListener('hashchange', load);
    return () => window.removeEventListener('hashchange', load);
  }, [path, unchecked, ask]);

  return state;
}
