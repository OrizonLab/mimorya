import type { ReactNode } from 'react';

/** The frame of the pages before sign-in: product name, tagline, then the page's content. */
export function AuthFrame({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-dvh items-center justify-center px-4 py-10">
      <div className="w-full max-w-sm">
        <div className="mb-6 text-center">
          <h1 className="text-2xl font-semibold tracking-tight">Mimorya</h1>
          <p className="mt-1 text-sm text-muted">Votre collaborateur IA d’entreprise</p>
        </div>
        {children}
      </div>
    </div>
  );
}
