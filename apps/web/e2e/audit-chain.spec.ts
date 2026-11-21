// The verification of the company's audit chain from the « Journal d’audit »
// tab (ADR-179): Paul verifies the chain, which the end-to-end server sealed
// at start, and keeps the anchor shown; verified again with it and with the
// same place under another seal, the first holds and the second does not, and
// no anchor to keep is then offered.
// The log traces both verifications, read again after reloading. Where the
// chain begins is said (ADR-219): here, no deletion by retention; on stand-in
// answers of the API, after the places retention deleted, and, for a start
// that does not hold its seal, never as if nothing had gone. The rules
// themselves are tested on the API (`crates/api/tests/audit_chain*.rs`,
// `audit_retention*.rs`).
import type { Locator, Page } from '@playwright/test';
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
  // No deletion by retention yet: the chain must begin at its first place.
  await expect(
    report.getByText(/^Aucune suppression par la rétention n’est enregistrée sous son sceau/),
  ).toBeVisible();
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

/** The demonstration company, whose anchors the stand-in answers name. */
const COMPANY = '00000000-0000-4000-8000-000000000001';
const SEAL = 'a'.repeat(64);

/** The stand-in answer `answer` to the verification of `anchors`, checked
 * to be what the page sends, as the API answers the anchors it is given. */
async function verifiedAs(
  page: Page,
  answer: AuditChainVerification,
  anchors: string[],
): Promise<Locator> {
  await page.route('**/api/admin/audit/verifications', async (route) => {
    expect(route.request().postDataJSON()).toEqual({ anchors });
    await route.fulfill({ json: answer });
  });
  await openAdmin(page, TAB);
  await page.getByLabel(/Ancres à vérifier/).fill(anchors.join('\n'));
  await page.getByRole('button', { name: 'Vérifier le journal' }).click();
  return page.getByRole('status', { name: 'Résultat de la vérification' });
}

// The end-to-end server keeps no entry five years old: the API's answer
// stands in for a chain whose oldest places retention deleted (ADR-198).
test('the verification says where a purged chain begins', async ({ page }) => {
  const kept = `${COMPANY}:1100:${SEAL}`;
  const answer: AuditChainVerification = {
    holds: true,
    verified: 30,
    unsealed: 0,
    last: { place: 1230, seal: SEAL, anchor: `${COMPANY}:1230:${SEAL}` },
    firstPlace: 1201,
    purged: 1200,
    broken: 0,
    breaks: [],
    anchors: [{ place: 1100, seal: SEAL, found: 'purged' }],
    verifiedAt: '2026-11-21T08:00:00Z',
  };
  const report = await verifiedAs(page, answer, [kept]);
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

// What the API answers once a start that retention sealed was moved without
// the key and a place deleted (`crates/api/tests/audit_retention_trust.rs`):
// the start counts for none, its places told missing, never said untouched.
test('a start that does not hold its seal is never said untouched', async ({ page }) => {
  const answer: AuditChainVerification = {
    holds: false,
    // As the scenario reads: the chain ends at place 5, which follows the
    // missing places with nothing verified before it to check it against.
    verified: 0,
    unsealed: 0,
    last: { place: 5, seal: SEAL, anchor: `${COMPANY}:5:${SEAL}` },
    firstPlace: 1,
    purged: 0,
    broken: 4,
    breaks: [{ kind: 'missing', from: 1, to: 4 }],
    anchors: [],
    verifiedAt: '2026-11-21T08:00:00Z',
  };
  const report = await verifiedAs(page, answer, []);
  await expect(report.getByText('Journal rompu')).toBeVisible();
  await expect(report.getByText('Places 1 à 4 : entrées supprimées.')).toBeVisible();
  await expect(
    report.getByText(
      'Aucune suppression par la rétention n’est enregistrée sous son sceau : la chaîne doit ' +
        'commencer à sa première place.',
    ),
  ).toBeVisible();
  await expect(report.getByText(/aucune entrée n’en a/)).toHaveCount(0);
  await noError(page);
});
