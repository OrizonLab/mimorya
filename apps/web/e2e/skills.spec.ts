// The company's skills, written from the administration (ADR-052): a skill and
// its first version, a second version made active, the first made active
// again (the way back), its description changed, read back after a reload,
// then the skill retired and deleted. Whatever happens, the test's skill is
// gone afterwards.
import type { SkillDetail, SkillList } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const NAME = 'relance-e2e';
const DESCRIPTION = 'Relancer un client dont l’intervention attend une pièce (e2e).';
const CHANGED = 'Relancer un client en attente de pièce (e2e).';
const WRITE = { 'X-Mimorya-Request': '1' };

test.beforeEach(({ page }) => {
  // Retiring and deleting ask for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test.afterEach(async ({ page }) => {
  const listed = await page.request.get('/api/admin/skills');
  // 401: the test never signed in, so changed nothing.
  if (listed.status() === 401) return;
  const { skills } = (await listed.json()) as SkillList;
  for (const skill of skills.filter((s) => s.name === NAME)) {
    if (skill.status === 'active') {
      await page.request.delete(`/api/admin/skills/${skill.id}/activation`, { headers: WRITE });
    }
    const deleted = await page.request.delete(`/api/admin/skills/${skill.id}`, { headers: WRITE });
    expect(deleted.status()).toBe(200);
  }
});

test('a skill is written, made active, rolled back, described anew, retired and deleted', async ({
  page,
}) => {
  await openAdmin(page, 'Compétences');
  await page.getByRole('button', { name: 'Nouvelle compétence' }).click();
  await page.getByLabel('Nom (lettres minuscules, chiffres et tirets)').fill(NAME);
  await page.getByLabel('Description (une ligne, lue par l’agent)').fill(DESCRIPTION);
  await page.getByLabel('Contenu (procédure en Markdown)').fill('# Relance\n1. Version une.');
  await page.getByLabel('Note de version (facultative)').fill('Première version');
  await page.getByRole('button', { name: 'Créer la compétence' }).click();
  // The demo company's skill is listed too: this one is read in its own row.
  const card = page
    .locator('div')
    .filter({ has: page.getByRole('button', { name: NAME, exact: true }) })
    .filter({ has: page.getByRole('button', { name: `Modifier la description de ${NAME}` }) })
    .last();

  await page.getByRole('button', { name: NAME, exact: true }).click();
  await expect(page.getByText('Première version')).toBeVisible();
  await page.getByRole('button', { name: 'Nouvelle version' }).click();
  // Written from the latest version.
  const content = page.getByLabel('Contenu (procédure en Markdown)');
  await expect(content).toHaveValue('# Relance\n1. Version une.');
  await content.fill('# Relance\n1. Version deux.');
  await page.getByRole('button', { name: 'Enregistrer la version' }).click();

  await page.getByRole('button', { name: 'Activer la version 2' }).click();
  await expect(card.getByText('v2 active')).toBeVisible();
  await page.getByRole('button', { name: 'Revenir à la version 1' }).click();
  await expect(card.getByText('v1 active')).toBeVisible();

  await page.getByRole('button', { name: `Modifier la description de ${NAME}` }).click();
  await page.getByLabel('Description (une ligne, lue par l’agent)').fill(CHANGED);
  await page.getByRole('button', { name: 'Enregistrer la description' }).click();
  await expect(page.getByText(CHANGED)).toBeVisible();
  await noError(page);

  // As the API recorded it, after a reload.
  await page.reload();
  await page.getByRole('tab', { name: 'Compétences' }).click();
  await expect(card.getByText('v1 active')).toBeVisible();
  await expect(page.getByText(CHANGED)).toBeVisible();
  const { skills } = (await (await page.request.get('/api/admin/skills')).json()) as SkillList;
  const id = skills.find((s) => s.name === NAME)?.id;
  const detail = (await (await page.request.get(`/api/admin/skills/${id}`)).json()) as SkillDetail;
  expect(detail.skill.description).toBe(CHANGED);
  expect(detail.versions.map((v) => [v.version, v.status, v.content])).toEqual([
    [2, 'retired', '# Relance\n1. Version deux.'],
    [1, 'active', '# Relance\n1. Version une.'],
  ]);

  await card.getByRole('button', { name: 'Retirer', exact: true }).click();
  await expect(card.getByText('v1 active')).toHaveCount(0);
  await expect(card.getByText('Retirée').first()).toBeVisible();
  await page.getByRole('button', { name: `Supprimer ${NAME}` }).click();
  await expect(page.getByRole('button', { name: NAME, exact: true })).toHaveCount(0);
});
