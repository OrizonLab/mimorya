// The record of processing activities in the administration (ADR-217,
// ADR-218): Paul reads the demonstration company's, drawn from its
// settings (its controller, its activities, the demonstration connector said
// fictitious, the durations the purge applies, what the company completes
// itself), and downloads it as a CSV file a spreadsheet opens, one row per
// recipient, each cell kept as text. What it names, and for whom alone, is
// tested on the API (`crates/api/tests/processing_register*.rs`).
import { readFile } from 'node:fs/promises';
import type {
  ProcessingRegisterView,
  RegisterActivity,
  RegisterRecipient,
} from '../lib/api/contract';
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
  expect(text.startsWith('\uFEFF"Responsable du traitement";"Traitement";"Finalité";')).toBe(true);
  const rows = text.trimEnd().split('\r\n');
  const company = '"Néo Maintenance (démo)";';
  expect(rows.some((row) => row.startsWith(`${company}"Assistance par l’agent";`))).toBe(true);
  expect(rows.some((row) => row.startsWith(`${company}"Journal d’audit";`))).toBe(true);
  await noError(page);
});

/** A recipient of the answer below. */
function recipient(name: string, role: string): RegisterRecipient {
  return {
    name,
    role,
    place: 'declaredByCompany',
    placeLabel: 'Service déclaré',
    chosenBy: 'company',
    chosenByLabel: 'Choisi par l’entreprise',
  };
}

/** An activity of the answer below. */
function activity(fields: Partial<RegisterActivity>): RegisterActivity {
  return {
    key: 'assistance',
    name: 'Traitement',
    purpose: 'Finalité',
    subjects: [],
    data: [],
    sensitive: null,
    recipients: [],
    retention: [],
    notes: [],
    ...fields,
  };
}

// No text of the platform begins as a formula would: the API's answer stands
// in for a company whose name and settings would, to check what the file
// keeps of them, cell by cell.
test('the downloaded file keeps each cell as text, one row per recipient', async ({ page }) => {
  const answer: ProcessingRegisterView = {
    controller: '=Acme; "Industrie"',
    drawnAt: '2026-11-20T23:30:00Z',
    activities: [
      activity({
        name: '+Sans destinataire',
        purpose: '-moins',
        subjects: ['@personnes', 'autres'],
        data: ['\tdonnées'],
        retention: ['\rdurée'],
      }),
      activity({
        key: 'connectors',
        name: 'Deux destinataires',
        recipients: [
          recipient('=HYPERLINK("x")', 'pour; « elle »'),
          recipient('Anthropic', 'Modèle'),
        ],
        notes: ['Une note'],
      }),
    ],
    measures: ['Mesure'],
    toComplete: ['À compléter'],
  };
  await page.route('**/api/admin/processing-register', (route) => route.fulfill({ json: answer }));
  await openAdmin(page, TAB);
  const downloading = page.waitForEvent('download');
  await page.getByRole('button', { name: 'Télécharger (CSV)' }).click();
  const download = await downloading;
  // Drawn at half past midnight in Paris: the day of Paris.
  expect(download.suggestedFilename()).toBe('registre-des-traitements-2026-11-21.csv');
  const text = await readFile(await download.path(), 'utf8');
  const controller = `"'=Acme; ""Industrie"""`;
  const declared = '"Service déclaré";"Choisi par l’entreprise"';
  expect(text.split('\r\n')).toEqual([
    '\uFEFF"Responsable du traitement";"Traitement";"Finalité";"Personnes concernées";' +
      '"Catégories de données";"Données sensibles";"Destinataire";' +
      '"Reçoit les données pour";"Localisation";"Choisi par";"Durées de conservation";' +
      '"Remarques"',
    `${controller};"'+Sans destinataire";"'-moins";"'@personnes\nautres";"'\tdonnées";"";` +
      `"Aucun hors de la plateforme";"";"";"";"'\rdurée";""`,
    `${controller};"Deux destinataires";"Finalité";"";"";"";"'=HYPERLINK(""x"")";` +
      `"pour; « elle »";${declared};"";"Une note"`,
    `${controller};"Deux destinataires";"Finalité";"";"";"";"Anthropic";"Modèle";` +
      `${declared};"";"Une note"`,
    '',
  ]);
  await noError(page);
});
