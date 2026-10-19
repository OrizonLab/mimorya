import type { ReactNode } from 'react';
import { Button } from './button';
import { ErrorNote, Spinner } from './feedback';

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
  if (state.error) {
    return (
      <div className="flex flex-col items-start gap-2">
        <ErrorNote message={state.error} />
        <Button variant="secondary" onClick={state.reload}>
          Réessayer
        </Button>
      </div>
    );
  }
  if (state.data === null) return <Spinner />;
  return (
    // `contents`: the content is laid out as if the fieldset were not there.
    <fieldset disabled={state.loading} aria-busy={state.loading} className="contents">
      {children(state.data)}
    </fieldset>
  );
}
