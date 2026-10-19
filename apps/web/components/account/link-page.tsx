'use client';

import type { ReactNode } from 'react';
import { Card, ErrorNote, Spinner } from '@/components/ui';
import { AuthFrame } from './auth-frame';
import type { LinkState } from './use-link-token';

/** A page opened from a one-time link: checking it, why it cannot be used, or what it opens. */
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
      {state.kind === 'ready' && children(state.token, state.view)}
    </AuthFrame>
  );
}
