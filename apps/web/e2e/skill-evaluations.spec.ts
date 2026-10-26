// A skill's version tried on its cases (ADR-103, ADR-104, ADR-105): Paul
// writes a case for a skill whose version 1 is active, renames it, asks for
// version 2 to be evaluated, follows it to its end, reads the regression it
// found case by case (the answers, the judge's issue, the tools called),
// declines to make version 2 active against it, then makes it active once he
// confirmed, deletes the case, and reads it all back as the API recorded it.
// Whatever happens, the test's skill is gone afterwards, its cases and
// evaluations with it.
import type { Created, SkillDetail, SkillEvaluations, SkillList } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const NAME = 'retards-e2e';
const WRITE = { 'X-Mimorya-Request': '1' };
/** What the server's model plays (`e2e_server/skill_evaluations.rs`). */
const ASKED = 'Quels retards faut-il signaler cette semaine ? (évaluation e2e)';
const V1 = '# Retards\n1. Citer chaque retard de la semaine.';
const V2 = '# Retards\n1. Ne citer aucun retard.';
const CITED = 'Deux retards : l’intervention de Lyon et celle de Nantes.';
const NONE_CITED = 'Aucun retard à signaler.';
const ISSUE = 'Les retards de la semaine ne sont pas cités.';

const dialogs: string[] = [];
/** Whether the next activation asked against a regression is declined. */
let declineActivation = false;

test.beforeEach(({ page }) => {
  dialogs.length = 0;
  declineActivation = false;
  // Deleting a case and activating against a regression ask for a confirmation.
  page.on('dialog', (dialog) => {
    dialogs.push(dialog.message());
    const declined = declineActivation && dialog.message().includes('Activer quand même');
    if (declined) declineActivation = false;
    void (declined ? dialog.dismiss() : dialog.accept());
  });
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

test('a version is evaluated on a case, its regression read, then made active once confirmed', async ({
  page,
}) => {
  await openAdmin(page, 'Compétences');
  // The skill, its version 1 active and its version 2, as written before.
  const created = await page.request.post('/api/admin/skills', {
    headers: WRITE,
    data: { name: NAME, description: 'Signaler les retards de la semaine (e2e).', content: V1 },
  });
  expect(created.status()).toBe(201);
  const { id } = (await created.json()) as Created;
  const skillPath = `/api/admin/skills/${id}`;
  for (const [path, method, data] of [
    [`${skillPath}/activation`, 'put', { version: 1 }],
    [`${skillPath}/versions`, 'post', { content: V2 }],
  ] as const) {
    const done = await page.request[method](path, { headers: WRITE, data });
    expect(done.ok()).toBe(true);
  }
  await page.reload();
  await page.getByRole('tab', { name: 'Compétences' }).click();
  await page.getByRole('button', { name: NAME, exact: true }).click();

  // A case, written then renamed.
  const cases = page.getByRole('region', { name: 'Cas d’évaluation' });
  await cases.getByRole('button', { name: 'Nouveau cas' }).click();
  await cases.getByLabel('Nom du cas').fill('Retards');
  await cases.getByLabel('Demande (posée comme le ferait un collaborateur)').fill(ASKED);
  await cases
    .getByLabel('Critères d’une bonne réponse (un par ligne, 8 au plus)')
    .fill('Cite chaque retard de la semaine.\nNomme les interventions concernées.');
  await cases.getByRole('button', { name: 'Ajouter le cas' }).click();
  await cases.getByRole('button', { name: 'Modifier le cas Retards' }).click();
  await cases.getByLabel('Nom du cas').fill('Retards de la semaine');
  await cases.getByRole('button', { name: 'Enregistrer le cas' }).click();
  await expect(cases.getByText('Nomme les interventions concernées.')).toBeVisible();

  // Version 2, chosen by default, tried beside version 1 and followed to its end.
  const evaluations = page.getByRole('region', { name: 'Évaluations' });
  await expect(evaluations.getByLabel('Version à évaluer')).toHaveValue('2');
  await evaluations.getByRole('button', { name: 'Lancer l’évaluation' }).click();
  const evaluation = evaluations.getByRole('listitem').filter({ hasText: 'Évaluation de la' });
  await expect(evaluation).toContainText('Terminée', { timeout: 30_000 });
  await expect(evaluation).toContainText('1 régression');
  await expect(evaluation).toContainText(
    'Version 2 : 0 cas réussi sur 1 · version de référence 1 : 1 sur 1',
  );
  await evaluation.getByRole('button', { name: /Voir les cas/ }).click();
  await expect(evaluation.getByText('Régression', { exact: true })).toBeVisible();
  await expect(evaluation.getByText(NONE_CITED)).toBeVisible();
  await expect(evaluation.getByText(ISSUE)).toBeVisible();
  await expect(evaluation.getByText(CITED)).toBeVisible();
  await expect(evaluation.getByText('Compétence retards-e2e v2')).toBeVisible();
  await expect(evaluation.getByText('Compétence retards-e2e v1')).toBeVisible();

  // Against its regression: declined, nothing changes.
  declineActivation = true;
  const activate = page.getByRole('button', { name: 'Activer la version 2' });
  await activate.click();
  await expect
    .poll(() => dialogs.join('\n'))
    .toContain('1 cas réussi par la version active ne l’est plus');
  await expect(activate).toBeEnabled();
  const kept = (await (await page.request.get(skillPath)).json()) as SkillDetail;
  expect(kept.skill.activeVersion).toBe(1);
  // Then made active, once confirmed.
  await activate.click();
  await expect(page.getByText('v2 active')).toBeVisible();

  await cases.getByRole('button', { name: 'Supprimer le cas Retards de la semaine' }).click();
  await expect(cases.getByText('Aucun cas')).toBeVisible();
  await noError(page);

  // As the API recorded it.
  const detail = (await (await page.request.get(skillPath)).json()) as SkillDetail;
  expect(detail.skill.activeVersion).toBe(2);
  expect(detail.cases).toEqual([]);
  const { evaluations: stored } = (await (
    await page.request.get(`${skillPath}/evaluations`)
  ).json()) as SkillEvaluations;
  expect(
    stored.map((e) => [e.version, e.status, e.passed, e.referencePassed, e.regressions]),
  ).toEqual([[2, 'completed', 0, 1, 1]]);
});
