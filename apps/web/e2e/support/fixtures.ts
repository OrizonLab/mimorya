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
  nadia: 'nadia.benali@neo-maintenance.example',
};

/** Reports into `found` what breaks the policy or throws in `context`'s pages. */
async function watch(context: BrowserContext, found: string[]) {
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

interface Fixtures {
  /** What broke the policy or threw so far, in any context of the test. */
  violations: string[];
  /** Opens another browser context (another person), watched like the first one. */
  newContext: () => Promise<BrowserContext>;
}

/**
 * Each test fails if a page of any of its contexts breaks the Content
 * Security Policy (ADR-030) or throws; requests to `FOREIGN_HOST` are
 * refused on purpose. Contexts come from `newContext`, which watches them,
 * so that none can be forgotten.
 */
export const test = base.extend<Fixtures>({
  violations: [
    async ({ context }, use) => {
      const found: string[] = [];
      await watch(context, found);
      await use(found);
      expect(found.filter((entry) => !entry.includes(FOREIGN_HOST))).toEqual([]);
    },
    { auto: true },
  ],
  newContext: async ({ browser, violations }, use) => {
    const opened: BrowserContext[] = [];
    await use(async () => {
      const context = await browser.newContext();
      await watch(context, violations);
      opened.push(context);
      return context;
    });
    // Closed before `violations` checks what they reported.
    await Promise.all(opened.map((context) => context.close()));
  },
});

export { expect };

export async function signIn(page: Page, email: string, password = DEMO_PASSWORD) {
  await page.goto('/login');
  await page.getByLabel('Adresse e-mail professionnelle').fill(email);
  await page.getByLabel('Mot de passe').fill(password);
  await page.getByRole('button', { name: 'Se connecter', exact: true }).click();
  await page.waitForURL('**/chat');
}

/**
 * No error message is shown, named if one is. Next.js's route announcer, an
 * alert too, is left out: after a client-side navigation it may read the
 * page's title, which goes blank for a moment while the page changes.
 */
export async function noError(page: Page) {
  const shown = page
    .getByRole('alert')
    .filter({ hasText: /\S/ })
    .and(page.locator(':not(#__next-route-announcer__)'));
  await expect(shown).toHaveText([], { useInnerText: true });
}
