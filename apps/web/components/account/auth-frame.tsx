import type { ReactNode } from 'react';
import { BrandMark } from '@/components/shell/brand-mark';

/** The frame of the pages before sign-in: the product's logo and name (ADR-222), tagline, then the page's content. */
export function AuthFrame({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-dvh items-center justify-center px-4 py-10">
      <div className="w-full max-w-sm">
        <div className="mb-6 text-center">
          <h1 className="text-2xl font-semibold tracking-tight">
            <BrandMark />
          </h1>
          <p className="mt-1 text-sm text-muted">Votre collaborateur IA d’entreprise</p>
        </div>
        {children}
      </div>
    </div>
  );
}
