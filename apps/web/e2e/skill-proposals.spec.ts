// Skills the agent proposes (ADR-100): Lucas asks the agent to propose a
// procedure, reads its exact text and validates it; Paul finds the new
// skill among the company's, a draft whose proposal awaits him, reads who
// it was proposed for and why, turns it down, reads it back after a
// reload, then deletes the skill. Whatever happens, the test's skill is
// gone afterwards.
import type { Page } from '@playwright/test';
import type { SkillList } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { demo, expect, noError, signIn, test } from './support/fixtures';

/** The question, the skill and the answer the server's model plays (`e2e_server/skill_proposals.rs`). */
const ASKED =
  'Propose aux administrateurs notre façon de relancer un fournisseur dont la pièce tarde.';
const NAME = 'relance-fournisseur';
const DESCRIPTION = 'Relancer un fournisseur dont la pièce commandée tarde.';
const RATIONALE = 'Procédure suivie avec succès dans cette conversation.';
const ANSWER =
  'La procédure de relance d’un fournisseur est proposée aux administrateurs des compétences.';
const WRITE = { 'X-Mimorya-Request': '1' };

test.beforeEach(({ page }) => {
  // Deleting asks for a confirmation.
  page.on('dialog', (dialog) => dialog.accept());
});

test.afterEach(async ({ page }) => {
  await signIn(page, demo.paul);
  const { skills } = (await (await page.request.get('/api/admin/skills')).json()) as SkillList;
  for (const skill of skills.filter((s) => s.name === NAME)) {
    if (skill.status === 'active') {
      await page.request.delete(`/api/admin/skills/${skill.id}/activation`, { headers: WRITE });
    }
    const deleted = await page.request.delete(`/api/admin/skills/${skill.id}`, { headers: WRITE });
    expect(deleted.status()).toBe(200);
  }
});

/** The skills tab, reloaded from the API, the test's skill opened. */
async function openTheSkill(page: Page) {
  await page.goto('/admin');
  await page.getByRole('tab', { name: 'Compétences' }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
  await page.getByRole('button', { name: NAME, exact: true }).click();
  return page.getByRole('listitem').filter({ hasText: 'Version 1' });
}

test('a skill the agent proposes is validated, then turned down by an administrator', async ({
  page,
}) => {
  await signIn(page, demo.lucas);
  await page.getByLabel('Votre message').fill(ASKED);
  await page.getByRole('button', { name: 'Envoyer' }).click();

  // Nothing leaves the conversation before Lucas validates its text.
  await expect(page.getByText('Validation requise')).toBeVisible({ timeout: 30_000 });
  await expect(
    page.getByText(
      `Proposer aux administrateurs des compétences la nouvelle compétence « ${NAME} »`,
    ),
  ).toBeVisible();
  await page.getByText('Paramètres exacts qui seront exécutés').click();
  await expect(page.getByText('Appeler le fournisseur, puis confirmer par écrit.')).toBeVisible();
  await page.getByRole('button', { name: 'Valider' }).click();
  await expect(page.getByText(ANSWER)).toBeVisible({ timeout: 30_000 });

  // Paul reads it among the skills, a draft whose proposal awaits him.
  await openAdmin(page, 'Compétences');
  await expect(page.getByRole('status')).toContainText(
    `Une proposition de l’agent attend votre décision : « ${NAME} »`,
  );
  const card = page
    .locator('div')
    .filter({ hasText: DESCRIPTION })
    .filter({
      has: page.getByRole('button', { name: NAME, exact: true }),
    });
  await expect(card.last()).toContainText('Brouillon');
  await expect(card.last()).toContainText('1 proposition à relire');
  const version = await openTheSkill(page);
  await expect(version).toContainText('Proposée');
  await expect(version).toContainText(/proposée par l’agent pour Lucas/);
  await expect(version).toContainText(RATIONALE);
  await expect(version).toContainText(`Description proposée : ${DESCRIPTION}`);
  await expect(version).toContainText('Retrouver la commande et sa date promise.');
  await version.getByRole('button', { name: 'Rejeter la version 1' }).click();
  await expect(version).toContainText('Rejetée');

  // As the API recorded it: turned down, nothing awaits any more.
  const reread = await openTheSkill(page);
  await expect(reread).toContainText('Rejetée');
  await expect(reread.getByRole('button', { name: 'Rejeter la version 1' })).toHaveCount(0);
  await expect(page.getByText('attend votre décision')).toHaveCount(0);
  await expect(page.getByText('proposition à relire')).toHaveCount(0);

  await page.getByRole('button', { name: `Supprimer ${NAME}` }).click();
  await expect(page.getByRole('button', { name: NAME, exact: true })).toHaveCount(0);
  await noError(page);
});
