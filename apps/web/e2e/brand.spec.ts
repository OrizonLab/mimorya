// The installation's brand (ADR-222), as the test server is started with
// it (playwright.config.ts): « Atelier Nova », its assistant « Nova », its
// accents and the logo and favicon of `e2e/brand`. The tab, the sign-in,
// the sidebar and the header show it, before any session as after; the
// accents are the interface's in each scheme; the images are served under
// a policy that lets them draw themselves and nothing else. What is
// refused (a colour people could not read, a file that lies about its
// type, an SVG that runs a script) stops the API: tested in Rust
// (`crates/core/src/brand`, `crates/api/src/config/brand.rs`,
// `crates/api/src/services/brand`).
import type { Page } from '@playwright/test';
import type { BrandView } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const PRODUCT = 'Atelier Nova';
const TITLE = `${PRODUCT} — collaborateur IA`;

/** The accent the page draws with, as the browser computes it. */
async function accent(page: Page): Promise<string> {
  return page.evaluate(() =>
    getComputedStyle(document.documentElement).getPropertyValue('--accent').trim(),
  );
}

test('the sign-in shows the brand, before any session', async ({ page }) => {
  await page.goto('/login');
  await expect(page).toHaveTitle(TITLE);
  const heading = page.getByRole('heading', { level: 1 });
  await expect(heading).toHaveText(PRODUCT);
  const logo = heading.locator('img');
  await expect(logo).toHaveAttribute('src', /^\/api\/brand\/logo\?v=[0-9a-f]{16}$/);
  expect(await logo.evaluate((image: HTMLImageElement) => image.naturalWidth)).toBeGreaterThan(0);
  const icon = page.locator('link[rel="icon"]');
  await expect(icon).toHaveAttribute('href', /^\/api\/brand\/favicon\?v=[0-9a-f]{16}$/);
  await expect(icon).toHaveAttribute('type', 'image/png');
  expect(await accent(page)).toBe('#6b2fa3');
  await page.emulateMedia({ colorScheme: 'dark' });
  expect(await accent(page)).toBe('#c4a3e8');
  await noError(page);
});

test('the application shows the product, its assistant keeps its own name', async ({ page }) => {
  await signIn(page, demo.marc);
  await expect(page).toHaveTitle(TITLE);
  const navigation = page.getByRole('navigation', { name: 'Navigation principale' });
  await expect(navigation.getByText(PRODUCT, { exact: true })).toBeVisible();
  await expect(navigation.locator('img[src^="/api/brand/logo"]')).toBeVisible();
  const brand = (await (await page.request.get('/api/brand')).json()) as BrandView;
  expect(brand).toMatchObject({
    productName: PRODUCT,
    assistantName: 'Nova',
    filePrefix: 'atelier-nova',
    light: { accent: '#6b2fa3', accentFg: '#ffffff' },
    dark: { accent: '#c4a3e8', accentFg: '#0b1219' },
    logo: { mediaType: 'image/svg+xml' },
    favicon: { mediaType: 'image/png' },
  });
  await noError(page);
});

test('the brand’s images draw themselves and nothing else', async ({ request }) => {
  const brand = (await (await request.get('/api/brand')).json()) as BrandView;
  for (const [kind, type] of [
    ['logo', 'image/svg+xml; charset=utf-8'],
    ['favicon', 'image/png'],
  ] as const) {
    const image = await request.get(`/api/brand/${kind}?v=${brand[kind]?.version}`);
    expect(image.status()).toBe(200);
    expect(image.headers()['content-type']).toBe(type);
    expect(image.headers()['x-content-type-options']).toBe('nosniff');
    const policy = image.headers()['content-security-policy'];
    expect(policy).toContain("default-src 'none'");
    expect(policy).toContain('sandbox');
    expect(image.headers()['cache-control']).toContain('immutable');
  }
});
