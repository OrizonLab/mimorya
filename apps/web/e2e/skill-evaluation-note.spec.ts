// An evaluation played without the connectors that reach outside the
// platform says so (ADR-153, ADR-156): the server leaves one of the weekly
// report at start (`e2e_server/evaluation_records.rs`), as a proposal of the
// agent's is played; Paul reads it in « Compétences », as the API recorded
// it. One that kept them says nothing (`skill-evaluations.spec.ts`). The
// rule itself is tested on the API (`crates/api/tests/skill_evaluation_tools.rs`).
import type { SkillEvaluations, SkillList } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

/** The demo company's skill (`db/src/seed/skills.rs`). */
const SKILL = 'synthese-hebdo-interventions';
const NOTE = 'Jouée sans les connecteurs qui joignent l’extérieur';

test('an evaluation played without what reaches outside says so', async ({ page }) => {
  await openAdmin(page, 'Compétences');
  await page.getByRole('button', { name: SKILL, exact: true }).click();
  const evaluation = page
    .getByRole('region', { name: 'Évaluations' })
    .getByRole('listitem')
    .filter({ hasText: 'Évaluation de la version 1' });
  await expect(evaluation).toContainText(NOTE);
  await expect(evaluation).toContainText('la version évaluée était une proposition de l’agent');
  await noError(page);

  const { skills } = (await (await page.request.get('/api/admin/skills')).json()) as SkillList;
  const skill = skills.find((s) => s.name === SKILL);
  const listed = (await (
    await page.request.get(`/api/admin/skills/${skill?.id}/evaluations`)
  ).json()) as SkillEvaluations;
  expect(listed.evaluations.map((e) => [e.version, e.status, e.insideOnly])).toEqual([
    [1, 'completed', true],
  ]);
});
