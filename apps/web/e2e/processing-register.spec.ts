// The record of processing activities in the administration (ADR-217,
// ADR-218): Paul reads the demonstration company's, drawn from its
// settings (its controller, its activities, the demonstration connector said
// fictitious, the durations the purge applies, what the company completes
// itself), and downloads it as a CSV file a spreadsheet opens, one row per
// recipient. What it names, and for whom alone, is tested on the API
// (`crates/api/tests/processing_register*.rs`).
import { readFile } from 'node:fs/promises';
import { openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';

const TAB = 'Registre des traitements';

test('the administrator reads the register and downloads it', async ({ page }) => {
  await openAdmin(page, TAB);
  const head = page.getByRole('region', { name: TAB });
  await expect(head.getByText('Néo Maintenance (démo)')).toBeVisible();
  const assistance = page.getByRole('region', { name: 'Assistance par l’agent' });
  await expect(assistance.getByText(/^Conversations sans activité depuis \d+ mois/)).toBeVisible();
  await expect(assistance.getByText(/^Données sensibles :/)).toBeVisible();
  const connectors = page.getByRole('region', { name: 'Connecteurs et comptes reliés' });
  await expect(connectors.getByText(/démonstration, données fictives/)).toBeVisible();
  const audit = page.getByRole('region', { name: 'Journal d’audit' });
  await expect(audit.getByText(/^5 ans/)).toBeVisible();
  await expect(page.getByRole('region', { name: 'Mesures de sécurité' })).toContainText(
    'Second facteur',
  );
  await expect(
    page.getByRole('region', { name: 'À compléter par votre entreprise' }),
  ).toContainText('Base légale de chaque traitement');

  const downloading = page.waitForEvent('download');
  await head.getByRole('button', { name: 'Télécharger (CSV)' }).click();
  const download = await downloading;
  expect(download.suggestedFilename()).toMatch(/^registre-des-traitements-\d{4}-\d{2}-\d{2}\.csv$/);
  const text = await readFile(await download.path(), 'utf8');
  expect(text.startsWith('\uFEFF"Traitement";"Finalité";')).toBe(true);
  const rows = text.trimEnd().split('\r\n');
  expect(rows.some((row) => row.startsWith('"Assistance par l’agent";'))).toBe(true);
  expect(rows.some((row) => row.startsWith('"Journal d’audit";'))).toBe(true);
  await noError(page);
});
