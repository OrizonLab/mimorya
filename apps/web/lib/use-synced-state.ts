'use client';

import { useState, type Dispatch, type SetStateAction } from 'react';

/**
 * A form field's state, taken from `source` and taken again whenever `source`
 * changes: a form shown while its list reloads ends up with what the list now
 * holds, without being rebuilt (its pending request and messages are kept).
 * `source` is compared by its JSON, which must identify it: the same for the
 * same value, from one render to the next, and different for another value.
 * Plain data only: strings, numbers, booleans, arrays and objects with keys
 * in a stable order; no function, Map, Set or `undefined` field.
 */
export function useSyncedState<T>(source: T): [T, Dispatch<SetStateAction<T>>] {
  const version = JSON.stringify(source);
  const [state, setState] = useState(source);
  const [seen, setSeen] = useState(version);
  if (seen !== version) {
    // Adjusted while rendering, as React recommends for state derived from props.
    setSeen(version);
    setState(source);
  }
  return [state, setState];
}
