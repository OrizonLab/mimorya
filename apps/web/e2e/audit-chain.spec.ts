// The verification of the company's audit chain from the « Journal d’audit »
// tab (ADR-179): Paul verifies the chain, which the end-to-end server sealed
// at start, and keeps the anchor shown; verified again with it and with the
// same place under another seal, the first holds and the second does not, and
// no anchor to keep is then offered.
// The log traces both verifications, read again after reloading. Where the
// chain begins is said: at its first place here; after the places that
// retention deleted on a stand-in answer of the API (ADR-219). The rules
// themselves are tested on the API (`crates/api/tests/audit_chain*.rs`,
// `audit_retention*.rs`).
import type { AuditChainVerification } from '../lib/api/contract';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const TAB = 'Journal d’audit';

test('an administrator verifies the audit chain against the anchor they kept', async ({ page }) => {
  await openAdmin(page, TAB);
  const verify = page.getByRole('button', { name: 'Vérifier le journal' });
  const report = page.getByRole('status', { name: 'Résultat de la vérification' });
  await verify.click();
  await expect(report.getByText('Journal intact')).toBeVisible();
  // Nothing five years old yet: the chain begins at its first place.
  await expect(report.getByText(/^La chaîne commence à sa première place/)).toBeVisible();
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

// The end-to-end server keeps no entry five years old: the API's answer
// stands in for a chain whose oldest places retention deleted (ADR-198).
test('the verification says where a purged chain begins', async ({ page }) => {
  const seal = 'a'.repeat(64);
  const answer: AuditChainVerification = {
    holds: true,
    verified: 30,
    unsealed: 0,
    last: {
      place: 1230,
      seal,
      anchor: `${'0'.repeat(8)}-0000-0000-0000-${'0'.repeat(12)}:1230:${seal}`,
    },
    firstPlace: 1201,
    purged: 1200,
    broken: 0,
    breaks: [],
    anchors: [{ place: 1100, seal, found: 'purged' }],
    verifiedAt: '2026-11-21T08:00:00Z',
  };
  await page.route('**/api/admin/audit/verifications', (route) => route.fulfill({ json: answer }));
  await openAdmin(page, TAB);
  await page.getByRole('button', { name: 'Vérifier le journal' }).click();
  const report = page.getByRole('status', { name: 'Résultat de la vérification' });
  await expect(report.getByText('Journal intact')).toBeVisible();
  await expect(
    report.getByText(
      'La chaîne commence à la place n° 1201 : 1200 entrée(s) de plus de cinq ans en ont été ' +
        'supprimée(s) par la rétention, le sceau de la dernière gardé pour vérifier la suite.',
    ),
  ).toBeVisible();
  await expect(report.getByRole('listitem')).toHaveText([
    'Ancre de la place 1100 : avant le début de la chaîne : supprimée avec les entrées de plus de cinq ans.',
  ]);
  await noError(page);
});
