// A run a stop of the service interrupted, taken up at startup (ADR-053):
// the end-to-end server leaves one, a search of interventions under way
// after a planning its user refused, and takes it up as the API does. Its
// live follow-up marks the lost step, and only it, and says where the run
// went on; the run then asks for its validation again. It is refused, even
// when the test fails, so that no validation waits.
import { refuseWaiting } from './support/approvals';
import { demo, expect, noError, signIn, test } from './support/fixtures';

/** The conversation the server left interrupted (`e2e_server/interrupted.rs`). */
const INTERRUPTED = 'Point de la semaine interrompu';
const RESUMED = 'Le service a redémarré : la demande a repris à sa dernière étape enregistrée.';

test.afterEach(async ({ page }) => {
  await refuseWaiting(page);
});

test('a run taken up after a stop of the service shows the step it lost', async ({ page }) => {
  await signIn(page, demo.marc);
  await page.getByRole('link', { name: INTERRUPTED }).click();

  // The search under way when the service stopped is marked lost; the run
  // went on from its last saved step, and now waits for a validation.
  const lost = page
    .getByRole('listitem')
    .filter({ hasText: 'Interrompue par un arrêt du service' });
  await expect(lost).toHaveCount(1, { timeout: 30_000 });
  await expect(lost).toContainText('Recherche d’interventions');
  await expect(lost.getByLabel('Interrompue')).toBeVisible();
  await expect(page.getByText(RESUMED)).toBeVisible();
  // The planning refused before the stop ended then: it is not lost.
  const refused = page
    .getByRole('listitem')
    .filter({ hasText: 'Action refusée par l’utilisateur' });
  await expect(refused).toHaveCount(1);
  await expect(refused).toContainText('Application d’un planning');
  await expect(refused).not.toContainText('Interrompue');
  await expect(page.getByText('Validation requise')).toBeVisible({ timeout: 30_000 });

  // Refused, the run ends without changing the planning; read again, the
  // conversation keeps its answer.
  await page.getByRole('button', { name: 'Refuser' }).click();
  const unchanged = 'Le planning est inchangé : la validation a été refusée.';
  await expect(page.getByText(unchanged)).toBeVisible({ timeout: 30_000 });
  await page.reload();
  await expect(page.getByText(unchanged)).toBeVisible();
  await expect(page.getByText(RESUMED)).toHaveCount(0);
  await noError(page);
});
