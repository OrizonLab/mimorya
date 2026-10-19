'use client';

import { Button, Card } from '@/components/ui';
import './globals.css';

/**
 * Shown when the root layout itself fails: it replaces the whole document,
 * so it brings its own `<html>` and styles. Only classes, no inline style,
 * as the Content Security Policy requires (ADR-030); no detail of the error,
 * which may hold server-side information.
 */
export default function GlobalError({ retry }: { error: unknown; retry: () => void }) {
  return (
    <html lang="fr">
      <body>
        <div className="flex min-h-dvh items-center justify-center px-4 py-10">
          <Card className="flex w-full max-w-sm flex-col gap-3 p-6 text-sm">
            <p className="font-medium">Une erreur est survenue</p>
            <p className="text-muted">
              La page n’a pas pu s’afficher. Réessayez ; si l’erreur persiste, prévenez votre
              administrateur.
            </p>
            <Button className="w-fit" onClick={() => retry()}>
              Réessayer
            </Button>
          </Card>
        </div>
      </body>
    </html>
  );
}
