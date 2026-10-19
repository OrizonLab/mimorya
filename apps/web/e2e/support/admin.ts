import type { Locator, Page } from '@playwright/test';
import { demo, expect, signIn } from './fixtures';

/** Signs in as the demo administrator and opens a tab of the administration. */
export async function openAdmin(page: Page, tab: string) {
  await signIn(page, demo.paul);
  await page.goto('/admin');
  await page.getByRole('tab', { name: tab }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
}

/** The innermost block showing `text` and a button named `button`: one card's row. */
export function cardWith(page: Page, text: string, button: string): Locator {
  return page
    .locator('div')
    .filter({ hasText: text })
    .filter({ has: page.getByRole('button', { name: button }) })
    .last();
}

/** Selects the option whose text contains `text` (unit names are indented). */
export async function selectContaining(select: Locator, text: string) {
  const value = await select.locator('option', { hasText: text }).first().getAttribute('value');
  await select.selectOption(value ?? '');
}
