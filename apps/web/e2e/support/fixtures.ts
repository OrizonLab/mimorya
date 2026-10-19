import { test as base, expect, type BrowserContext, type Page } from '@playwright/test';

/** Every demo account's password (seed of the end-to-end database). */
export const DEMO_PASSWORD = 'demo-mimorya';

/** A host that may appear in a refused request on purpose. */
export const FOREIGN_HOST = 'exemple.invalid';

export const demo = {
  paul: 'paul.renaud@neo-maintenance.example',
  marc: 'marc.lefevre@neo-maintenance.example',
  lucas: 'lucas.petit@neo-maintenance.example',
  hugo: 'hugo.garnier@neo-maintenance.example',
};

/** Reports into `found` what breaks the policy or throws in `context`'s pages. */
export async function watch(context: BrowserContext, found: string[]) {
  await context.addInitScript(() => {
    document.addEventListener('securitypolicyviolation', (event) => {
      console.error(`CSP violation: ${event.violatedDirective} ${event.blockedURI}`);
    });
  });
  context.on('console', (message) => {
    if (message.text().startsWith('CSP violation')) found.push(message.text());
  });
  context.on('weberror', (error) => found.push(`page error: ${error.error().message}`));
}

/**
 * Each test fails if a page of its context, or of one it `watch`es, breaks
 * the Content Security Policy (ADR-030) or throws; requests to
 * `FOREIGN_HOST` are refused on purpose.
 */
export const test = base.extend<{ violations: string[] }>({
  violations: [
    async ({ context }, use) => {
      const found: string[] = [];
      await watch(context, found);
      await use(found);
      expect(found.filter((entry) => !entry.includes(FOREIGN_HOST))).toEqual([]);
    },
    { auto: true },
  ],
});

export { expect };

export async function signIn(page: Page, email: string, password = DEMO_PASSWORD) {
  await page.goto('/login');
  await page.getByLabel('Adresse e-mail professionnelle').fill(email);
  await page.getByLabel('Mot de passe').fill(password);
  await page.getByRole('button', { name: 'Se connecter' }).click();
  await page.waitForURL('**/chat');
}
