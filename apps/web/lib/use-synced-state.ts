'use client';

import { useState, type Dispatch, type SetStateAction } from 'react';

/**
 * A form field's state, taken from `source` and taken again whenever `source`
 * changes: a form shown while its list reloads ends up with what the list now
 * holds, without being rebuilt (its pending request and its focus are kept).
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
