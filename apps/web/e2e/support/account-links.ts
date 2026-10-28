// What the end-to-end tests do alike with the applications people link
// their accounts at, Microsoft 365's and Google Workspace's (ADR-118,
// ADR-124): the connector of a type read back from the API, with the link a
// person keeps there; the link made through the test provider and back; and
// the connectors of a type deleted after each test, with their links.
import type { Locator, Page } from '@playwright/test';
import type { AccountLinks, ConnectorView, Connectors } from '../../lib/api/contract';
import { expect } from './fixtures';

/** The connector types people link an account at. */
export type LinkType = 'microsoft365' | 'google_workspace';

const WRITE = { 'X-Mimorya-Request': '1' };

/** Deletes the connectors of this type a test declared, with their links. */
export async function deleteConnectorsOf(page: Page, type: LinkType) {
  const listed = await page.request.get('/api/admin/connectors');
  // 401: the test never signed in; 403: not as an administrator. Either
  // way, it declared nothing.
  if (listed.status() === 401 || listed.status() === 403) return;
  expect(listed.status()).toBe(200);
  const { instances } = (await listed.json()) as Connectors;
  for (const connector of instances.filter((c) => c.type === type)) {
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
}

/** The company's connector of this type as the API records it, and the
 * whole listing. */
export async function storedOf(
  page: Page,
  type: LinkType,
): Promise<{ connector?: ConnectorView; listing: string }> {
  const listed = await page.request.get('/api/admin/connectors');
  expect(listed.status()).toBe(200);
  const listing = await listed.text();
  const { instances } = JSON.parse(listing) as Connectors;
  return { connector: instances.find((c) => c.type === type), listing };
}

/** The person's link at the connector of this type, as the API lists it
 * to them. */
export async function linkOfType(page: Page, type: LinkType) {
  const listed = await page.request.get('/api/account/links');
  expect(listed.status()).toBe(200);
  const { connectors } = (await listed.json()) as AccountLinks;
  return connectors.find((c) => c.type === type);
}

/** Clicks the section's link button, through the provider and back. */
export async function linkThrough(page: Page, section: Locator, button: string) {
  const back = page.waitForResponse((r) => r.url().includes('/api/account/links/callback'));
  await section.getByRole('button', { name: button }).click();
  await back;
}
