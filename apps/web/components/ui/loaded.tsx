import type { ReactNode } from 'react';
import { ErrorNote, Spinner } from './feedback';

/** Renders the error, a spinner while loading, then the loaded content. */
export function Loaded<T>({
  state,
  children,
}: {
  state: { data: T | null; error: string | null };
  children: (data: T) => ReactNode;
}) {
  if (state.error) return <ErrorNote message={state.error} />;
  if (state.data === null) return <Spinner />;
  return <>{children(state.data)}</>;
}
