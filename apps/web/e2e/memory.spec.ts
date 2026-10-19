// A user's own memory: what they add is kept, shown in their personal space,
// and deleted on request (right to erasure).
import { demo, expect, signIn, test } from './support/fixtures';

test('a user adds a personal memory, then deletes it', async ({ page }) => {
  const title = 'Format des rapports (e2e)';
  await signIn(page, demo.marc);
  await page.goto('/memory');
  await expect(page.getByText('Chargement…')).toHaveCount(0);
  await page.getByLabel('Intitulé').fill(title);
  await page.getByLabel('Contenu').fill('Rapports en PDF, une page au plus');
  await page.getByRole('button', { name: 'Ajouter', exact: true }).click();
  await expect(page.getByText(title, { exact: true })).toBeVisible();
  await expect(page.getByText('Rapports en PDF, une page au plus')).toBeVisible();

  await page.getByRole('button', { name: `Supprimer « ${title} »` }).click();
  await expect(page.getByText(title, { exact: true })).toHaveCount(0);
});
