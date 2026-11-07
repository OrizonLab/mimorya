// The verification of the company's audit chain from the « Journal d’audit »
// tab (ADR-179): Paul verifies the chain, which the end-to-end server sealed
// at start, and keeps the anchor shown; verified again with it and with the
// same place under another seal, the first holds and the second does not, and
// no anchor to keep is then offered.
// The log traces both verifications, read again after reloading. The rules
// themselves are tested on the API (`crates/api/tests/audit_chain*.rs`).
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const TAB = 'Journal d’audit';

test('an administrator verifies the audit chain against the anchor they kept', async ({ page }) => {
  await openAdmin(page, TAB);
  const verify = page.getByRole('button', { name: 'Vérifier le journal' });
  const report = page.getByRole('status', { name: 'Résultat de la vérification' });
  await verify.click();
  await expect(report.getByText('Journal intact')).toBeVisible();
  await noError(page);
  const anchor = ((await report.getByLabel('Ancre à conserver').textContent()) ?? '').trim();
  expect(anchor).toMatch(/^[0-9a-f-]{36}:[1-9][0-9]*:[0-9a-f]{64}$/);
  const [company, place] = anchor.split(':');
  const resealed = `${company}:${place}:${'0'.repeat(64)}`;

  // Blank lines left out, each anchor checked in its order.
  await page.getByLabel(/Ancres à vérifier/).fill(`${anchor}\n\n${resealed}\n`);
  await verify.click();
  await expect(report.getByText('Ancre non tenue')).toBeVisible();
  // A chain that does not hold gives no anchor to keep.
  await expect(report.getByLabel('Ancre à conserver')).toHaveCount(0);
  await expect(report.getByText(/gardez les ancres que vous aviez/)).toBeVisible();
  const anchors = report.getByRole('listitem');
  await expect(anchors).toHaveText([
    `Ancre de la place ${place} : tenue.`,
    `Ancre de la place ${place} : sceau différent : la chaîne a été scellée à nouveau depuis cette place.`,
  ]);

  // Both verifications traced, as the log reads them again.
  await page.reload();
  await page.getByRole('tab', { name: TAB }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
  const traced = page.getByRole('row').filter({ hasText: 'audit.chain_verified' });
  await expect(traced.filter({ hasText: 'Succès' }).first()).toBeVisible();
  await expect(
    traced.filter({ hasText: 'Échec' }).filter({ hasText: 'Une ancre n’est plus tenue' }).first(),
  ).toBeVisible();
  await noError(page);
});
