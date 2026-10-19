'use client';

import { useEffect } from 'react';
import { keepFocusAcrossDisabling } from '@/lib/focus-keeper';

/** Installs the focus keeper on every page (ADR-043). */
export function FocusKeeper() {
  useEffect(() => keepFocusAcrossDisabling(), []);
  return null;
}
