'use client';

import Link from 'next/link';
import type { ReactNode } from 'react';
import { Card, ErrorNote, Spinner } from '@/components/ui';
import { AuthFrame } from './auth-frame';
import type { LinkState } from './use-link-token';

/**
 * A page opened from a one-time link: checking it, why it cannot be used,
 * that it asks to sign in first, or what it opens.
 */
export function LinkPage<T>({
  state,
  children,
}: {
  state: LinkState<T>;
  children: (token: string, view: T) => ReactNode;
}) {
  return (
    <AuthFrame>
      {state.kind === 'loading' && (
        <Card className="flex justify-center p-6">
          <Spinner />
        </Card>
      )}
      {state.kind === 'invalid' && (
        <Card className="p-6">
          <ErrorNote message={state.message} />
        </Card>
      )}
      {state.kind === 'signed-out' && (
        <Card className="flex flex-col gap-3 p-6">
          <p className="text-sm">
            Connectez-vous d’abord, puis rouvrez ce lien tel que vous l’avez reçu.
          </p>
          <Link href="/login" className="text-sm font-medium text-accent underline">
            Se connecter
          </Link>
        </Card>
      )}
      {state.kind === 'ready' && children(state.token, state.view)}
    </AuthFrame>
  );
}
