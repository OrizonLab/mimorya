// What the company's skill evaluations may cost over thirty days (ADR-154,
// ADR-155): Paul sets the limit in « Modèle d'IA », read back once the page
// is reloaded, sees what was spent there (what the server wrote at start,
// `e2e_server/evaluation_records.rs`) and beside the evaluations of a skill
// in « Compétences », then goes back to the platform's limit. The same
// amount written otherwise saves nothing. Whatever happens, the platform's
// limit is set again, to leave the demo company as the next files find it.
// The rules themselves are tested on the API
// (`crates/api/tests/evaluation_limits.rs`).
import type { Page } from '@playwright/test';
import type { EvaluationSettingsView, SkillEvaluations, SkillList } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
const SETTINGS = '/api/admin/evaluation-settings';
/** The demo company's skill (`db/src/seed/skills.rs`). */
const SKILL = 'synthese-hebdo-interventions';

/** Who the test signed in, for its cleanup. */
const left: { paul?: Page } = {};

test.afterEach(async () => {
  if (left.paul) {
    const reset = await left.paul.request.delete(SETTINGS, { headers: WRITE });
    expect(reset.status()).toBe(200);
  }
  left.paul = undefined;
});

async function openTheTab(page: Page) {
  await page.reload();
  await page.getByRole('tab', { name: 'Modèle d’IA' }).click();
}

test('the company sets what its skill evaluations may cost', async ({ page }) => {
  await openAdmin(page, 'Modèle d’IA');
  left.paul = page;
  const form = page.getByRole('form', { name: 'Limite des évaluations' });
  const cost = form.getByLabel('Coût au plus sur 30 jours (USD)');
  await expect(cost).toHaveValue('50');
  await expect(form.getByText('Limite de la plateforme, faute de')).toBeVisible();
  // Amounts as French reads them, a no-break space before the currency.
  await expect(form.getByText(/^Dépensé sur les 30 derniers jours : 1,234\s\$US\.$/)).toBeVisible();
  const save = form.getByRole('button', { name: 'Enregistrer' });
  await expect(save).toBeDisabled();
  // The limit saved, written otherwise: nothing to save.
  await cost.fill('50.00');
  await expect(save).toBeDisabled();
  await cost.fill('2.5');
  await save.click();
  // Anchored and case-sensitive: the platform's message holds these words too.
  await expect(form.getByText(/^Limite propre à votre entreprise\./)).toBeVisible();

  // Read back from the API once the page is reloaded.
  await openTheTab(page);
  await expect(cost).toHaveValue('2.5');
  await cost.fill('2.50');
  await expect(save).toBeDisabled();
  const stored = (await (await page.request.get(SETTINGS)).json()) as EvaluationSettingsView;
  expect([stored.maxCostMicros, stored.own]).toEqual([2_500_000, true]);
  await noError(page);

  // Beside the evaluations of a skill, with what was spent.
  await page.getByRole('tab', { name: 'Compétences' }).click();
  await page.getByRole('button', { name: SKILL, exact: true }).click();
  const evaluations = page.getByRole('region', { name: 'Évaluations' });
  await expect(evaluations).toContainText(
    /Évaluations de l’entreprise sur trente jours : 1,234\s\$US dépensés, pour une limite de 2,500\s\$US\./,
  );
  const { skills } = (await (await page.request.get('/api/admin/skills')).json()) as SkillList;
  const skill = skills.find((s) => s.name === SKILL);
  const listed = (await (
    await page.request.get(`/api/admin/skills/${skill?.id}/evaluations`)
  ).json()) as SkillEvaluations;
  expect([listed.spentMicros, listed.maxCostMicros]).toEqual([1_234_000, 2_500_000]);

  // Back to the platform's limit.
  await openTheTab(page);
  page.once('dialog', (dialog) => void dialog.accept());
  await form.getByRole('button', { name: 'Revenir à la limite de la plateforme' }).click();
  await expect(form.getByText('Limite de la plateforme, faute de')).toBeVisible();
  await openTheTab(page);
  await expect(cost).toHaveValue('50');
  await expect(
    form.getByRole('button', { name: 'Revenir à la limite de la plateforme' }),
  ).toHaveCount(0);
  await noError(page);
});
