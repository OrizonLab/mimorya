import type { Metadata, Viewport } from 'next';
import { connection } from 'next/server';
import type { ReactNode } from 'react';
import './globals.css';

export const metadata: Metadata = {
  title: 'Mimorya — collaborateur IA',
  description: 'Votre collaborateur IA d’entreprise',
};

export const viewport: Viewport = {
  width: 'device-width',
  initialScale: 1,
};

export default async function RootLayout({ children }: { children: ReactNode }) {
  // Rendered per request: the Content Security Policy nonce (proxy.ts) is new
  // for every page, which a page built in advance could not carry.
  await connection();
  return (
    <html lang="fr">
      <body>{children}</body>
    </html>
  );
}
