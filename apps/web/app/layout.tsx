import type { Metadata, Viewport } from 'next';
import { headers } from 'next/headers';
import { connection } from 'next/server';
import type { ReactNode } from 'react';
import { BrandProvider } from '@/components/shell/brand-context';
import { FocusKeeper } from '@/components/shell/focus-keeper';
import { brandImageUrl, brandStyle } from '@/lib/brand';
import { readBrand } from '@/lib/brand-server';
import './globals.css';

/** The tab's title and icon, the installation's brand (ADR-222). */
export async function generateMetadata(): Promise<Metadata> {
  const brand = await readBrand();
  return {
    title: `${brand.productName} — collaborateur IA`,
    description: 'Votre collaborateur IA d’entreprise',
    icons: brand.favicon
      ? { icon: [{ url: brandImageUrl('favicon', brand.favicon), type: brand.favicon.mediaType }] }
      : undefined,
  };
}

export const viewport: Viewport = {
  width: 'device-width',
  initialScale: 1,
};

/** The nonce of this page's policy (proxy.ts), which its inline style must carry. */
async function styleNonce(): Promise<string | undefined> {
  const policy = (await headers()).get('content-security-policy') ?? '';
  return /'nonce-([^']+)'/.exec(policy)?.[1];
}

export default async function RootLayout({ children }: { children: ReactNode }) {
  // Rendered per request: the Content Security Policy nonce (proxy.ts) is new
  // for every page, which a page built in advance could not carry.
  await connection();
  const brand = await readBrand();
  const style = brandStyle(brand);
  const nonce = style ? await styleNonce() : undefined;
  return (
    <html lang="fr">
      <head>{style && <style nonce={nonce}>{style}</style>}</head>
      <body>
        <FocusKeeper />
        <BrandProvider brand={brand}>{children}</BrandProvider>
      </body>
    </html>
  );
}
