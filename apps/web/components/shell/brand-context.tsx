'use client';

import { createContext, useContext, type ReactNode } from 'react';
import type { BrandView } from '@/lib/api';
import { DEFAULT_BRAND } from '@/lib/brand';

const BrandContext = createContext<BrandView>(DEFAULT_BRAND);

/** Gives every page the brand the layout read from the API (ADR-222). */
export function BrandProvider({ brand, children }: { brand: BrandView; children: ReactNode }) {
  return <BrandContext.Provider value={brand}>{children}</BrandContext.Provider>;
}

export function useBrand(): BrandView {
  return useContext(BrandContext);
}
