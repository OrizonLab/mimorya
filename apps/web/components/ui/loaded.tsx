import type { ReactNode } from 'react';
import { Failure } from './failure';
import { Spinner } from './feedback';

/**
 * Renders the error, a spinner while loading, then the loaded content. While
 * the content is reloaded, its controls are disabled: nothing is changed from
 * data the reload may replace (ADR-042).
 */
export function Loaded<T>({
  state,
  children,
}: {
  state: { data: T | null; error: string | null; loading: boolean; reload: () => void };
  children: (data: T) => ReactNode;
}) {
  if (state.error) return <Failure message={state.error} onRetry={state.reload} />;
  if (state.data === null) return <Spinner />;
  return (
    // `contents`: the content is laid out as if the fieldset were not there.
    <fieldset disabled={state.loading} aria-busy={state.loading} className="contents">
      {children(state.data)}
    </fieldset>
  );
}
