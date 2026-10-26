// Skills the agent proposes (ADR-100, ADR-101): Lucas asks the agent to propose a
// procedure, reads its exact text and validates it; Paul finds the new
// skill among the company's, a draft whose proposal awaits him, reads who
// it was proposed for and why, reads its content unfolded, is reminded to
// read it before evaluating it on a case and declines, turns it down, reads
// it back after a reload, finds a new version of his own starts empty
// rather than from the proposal, then deletes the skill. Whatever happens,
// the test's skill is gone afterwards.
import type { Page } from '@playwright/test';
import type { SkillEvaluations, SkillList } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
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

const dialogs: string[] = [];

test.beforeEach(({ page }) => {
  dialogs.length = 0;
  // Deleting asks for a confirmation; evaluating a proposal too, declined here.
  page.on('dialog', (dialog) => {
    dialogs.push(dialog.message());
    void (dialog.message().includes('proposition de l’agent') ? dialog.dismiss() : dialog.accept());
  });
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
  const card = cardWith(page, DESCRIPTION, NAME);
  await expect(card).toContainText('Brouillon');
  await expect(card).toContainText('1 proposition à relire');
  const version = await openTheSkill(page);
  await expect(version).toContainText('Proposée');
  await expect(version).toContainText(/proposée par l’agent pour Lucas/);
  await expect(version).toContainText(RATIONALE);
  await expect(version).toContainText(`Description proposée : ${DESCRIPTION}`);
  // Awaiting a decision, its content is unfolded.
  await expect(version.getByText('Retrouver la commande et sa date promise.')).toBeVisible();
  // Read before it is evaluated: declined, nothing is asked for.
  const cases = page.getByRole('region', { name: 'Cas d’évaluation' });
  await cases.getByRole('button', { name: 'Nouveau cas' }).click();
  await cases.getByLabel('Nom du cas').fill('Pièce en retard');
  await cases
    .getByLabel('Demande (posée comme le ferait un collaborateur)')
    .fill('La pièce du fournisseur n’est pas arrivée : que faire ?');
  await cases
    .getByLabel('Critères d’une bonne réponse (un par ligne, 8 au plus)')
    .fill('Propose d’appeler le fournisseur.');
  await cases.getByRole('button', { name: 'Ajouter le cas' }).click();
  const evaluations = page.getByRole('region', { name: 'Évaluations' });
  await expect(evaluations).toContainText('Proposition de l’agent : relisez son contenu');
  await evaluations.getByRole('button', { name: 'Lancer l’évaluation' }).click();
  await expect.poll(() => dialogs.join('\n')).toContain('Avez-vous relu son contenu ?');
  await expect(evaluations).toContainText('Aucune évaluation.');
  await version.getByRole('button', { name: 'Rejeter la version 1' }).click();
  await expect(version).toContainText('Rejetée');

  // As the API recorded it: turned down, nothing awaits any more, nothing
  // was asked to be evaluated.
  const reread = await openTheSkill(page);
  await expect(reread).toContainText('Rejetée');
  const { skills } = (await (await page.request.get('/api/admin/skills')).json()) as SkillList;
  const proposed = skills.find((s) => s.name === NAME);
  const asked = await page.request.get(`/api/admin/skills/${proposed?.id}/evaluations`);
  expect(((await asked.json()) as SkillEvaluations).evaluations).toEqual([]);
  await expect(page.getByRole('region', { name: 'Évaluations' })).toContainText(
    'Aucune évaluation.',
  );
  await expect(reread.getByRole('button', { name: 'Rejeter la version 1' })).toHaveCount(0);
  await expect(page.getByText('attend votre décision')).toHaveCount(0);
  await expect(page.getByText('proposition à relire')).toHaveCount(0);

  // A version of his own starts from what an administrator wrote: nothing.
  await page.getByRole('button', { name: 'Nouvelle version' }).click();
  await expect(page.getByLabel('Contenu (procédure en Markdown)')).toHaveValue('');
  await page.getByRole('button', { name: 'Annuler' }).click();

  await page.getByRole('button', { name: `Supprimer ${NAME}` }).click();
  await expect(page.getByRole('button', { name: NAME, exact: true })).toHaveCount(0);
  await noError(page);
});
