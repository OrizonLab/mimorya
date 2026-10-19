import type { Page } from '@playwright/test';

/**
 * Holds the reads (GET) of `path` once armed: `arm()`, called before what
 * sends the read, resolves when it is sent; `release()` lets it through.
 */
export async function holdList(page: Page, path: string) {
  let held = Promise.resolve();
  let release = () => {};
  await page.route(`**${path}`, async (route) => {
    if (route.request().method() === 'GET') await held;
    await route.continue().catch((error: unknown) => {
      // A read the page gave up on (its time ran out) is no longer there.
      if (route.request().failure() === null) throw error;
    });
  });
  return {
    arm: () => {
      held = new Promise<void>((done) => (release = done));
      return page.waitForRequest((r) => r.method() === 'GET' && r.url().endsWith(path));
    },
    release: () => release(),
  };
}
