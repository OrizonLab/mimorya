// A read of the API left without answer gives up after READ_TIMEOUT_MS: the
// screen says so and offers to try again, rather than stay locked or claim
// there is nothing (ADR-044). The page's clock is moved on instead of waited
// for; each test holds the read until it lets it through. The role a test
// creates is deleted, even when it fails.
import { READ_TIMEOUT_MS } from '../lib/api/client';
import type { Roles } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { holdList } from './support/hold';

const ROLE = 'Délai e2e';
const NO_ANSWER = 'Le serveur ne répond pas. Réessayez dans un instant.';

test.beforeEach(async ({ page }) => {
  await page.clock.install();
});

test.afterEach(async ({ page }) => {
  const listed = await page.request.get('/api/admin/roles');
  if (listed.status() !== 200) return;
  const { roles } = (await listed.json()) as Roles;
  for (const role of roles.filter((r) => r.name === ROLE)) {
    const deleted = await page.request.delete(`/api/admin/roles/${role.id}`, {
      headers: { 'X-Mimorya-Request': '1' },
    });
    expect(deleted.status()).toBe(200);
  }
});

test('a reload left without answer gives way to a retry', async ({ page }) => {
  page.on('dialog', (dialog) => dialog.accept());
  await openAdmin(page, 'Rôles');
  await page.getByRole('button', { name: 'Nouveau rôle' }).click();
  await page.getByLabel('Nom').fill(ROLE);
  await page.getByRole('checkbox', { name: 'Consulter les plannings' }).check();
  await page.getByRole('button', { name: 'Créer le rôle' }).click();
  const remove = page.getByRole('button', { name: `Supprimer ${ROLE}` });
  await expect(remove).toBeEnabled();

  const roles = await holdList(page, '/api/admin/roles');
  const reload = roles.arm();
  await remove.click();
  await reload;
  const create = page.getByRole('button', { name: 'Nouveau rôle' });
  await expect(create).toBeDisabled();
  // Past its time, the read fails: the screen is no longer locked on it...
  await page.clock.fastForward(READ_TIMEOUT_MS);
  await expect(page.getByText(NO_ANSWER)).toBeVisible();
  roles.release();
  // ...and can be loaded again.
  await page.getByRole('button', { name: 'Réessayer' }).click();
  await expect(create).toBeEnabled();
  await expect(page.getByText(ROLE, { exact: true })).toHaveCount(0);
  await noError(page);
});

test('the conversations list says it failed, never that it is empty', async ({ page }) => {
  await signIn(page, demo.paul);
  const list = await holdList(page, '/api/conversations');
  const read = list.arm();
  await page.goto('/chat');
  await read;
  await page.clock.fastForward(READ_TIMEOUT_MS);
  await expect(page.getByText(NO_ANSWER)).toBeVisible();
  await expect(page.getByText('Aucune conversation pour le moment.')).toHaveCount(0);
  list.release();
  await page.getByRole('button', { name: 'Réessayer' }).click();
  await expect(page.getByText(NO_ANSWER)).toHaveCount(0);
  await expect(page.getByRole('button', { name: 'Réessayer' })).toHaveCount(0);
  await noError(page);
});

test('the session read says it failed and can be tried again', async ({ page }) => {
  await signIn(page, demo.paul);
  const session = await holdList(page, '/api/auth/me');
  const read = session.arm();
  await page.goto('/admin');
  await read;
  await page.clock.fastForward(READ_TIMEOUT_MS);
  await expect(page.getByText(NO_ANSWER)).toBeVisible();
  session.release();
  await page.getByRole('button', { name: 'Réessayer' }).click();
  await expect(page.getByRole('heading', { name: 'Administration' })).toBeVisible();
  await noError(page);
});
