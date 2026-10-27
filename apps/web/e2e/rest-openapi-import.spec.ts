// A REST API's operations read from its OpenAPI description (ADR-114,
// ADR-115): Paul chooses a description in the REST API form. One that cannot
// be read is said. A YAML one, converted in the browser, offers its
// operations with what was left aside; he checks one, which the form then
// shows like any other, its address filled; reading it again, the operation
// already added cannot be checked twice; he adds another. Saved, the API
// holds both, read back from the API. Then an API one operation short of
// fifty, changed: a JSON description offers two, of which only one can be
// checked; saved, it holds fifty. Nothing is called. Whatever happens, the
// tests' APIs are deleted.
import type { Page } from '@playwright/test';
import type { ConnectorView, Connectors, NewRestConnector } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { expect, FOREIGN_HOST, noError, test } from './support/fixtures';

const API = 'Assistance e2e';
const FULL = 'Pleine e2e';
const URL = `https://api.${FOREIGN_HOST}/v2`;
const WRITE = { 'X-Mimorya-Request': '1' };

const DESCRIPTION = `openapi: 3.0.3
info:
  title: Assistance
  version: "1"
servers:
  - url: ${URL}
paths:
  /tickets/{id}:
    get:
      operationId: getTicket
      summary: Lire un ticket
      parameters:
        - name: id
          in: path
          required: true
          schema:
            type: string
  /tickets:
    post:
      operationId: createTicket
      summary: Créer un ticket
      requestBody:
        required: true
        content:
          application/json:
            schema:
              type: object
              required: [title]
              properties:
                title:
                  type: string
  /sessions:
    post:
      operationId: signIn
      parameters:
        - name: X-Tenant
          in: header
          required: true
          schema:
            type: string
`;

test.afterEach(async ({ page }) => {
  const listed = await page.request.get('/api/admin/connectors');
  // 401: the test never signed in, so changed nothing.
  if (listed.status() === 401) return;
  expect(listed.status()).toBe(200);
  const { instances } = (await listed.json()) as Connectors;
  for (const connector of instances.filter((c) => c.name === API || c.name === FULL)) {
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

/** A test's API as the API records it. */
async function stored(page: Page, name = API): Promise<ConnectorView | undefined> {
  const listed = await page.request.get('/api/admin/connectors');
  expect(listed.status()).toBe(200);
  const { instances } = (await listed.json()) as Connectors;
  return instances.find((c) => c.name === name);
}

const file = (name: string, text: string) => ({
  name,
  mimeType: 'application/yaml',
  buffer: Buffer.from(text),
});

test('operations are read from an OpenAPI description and added', async ({ page }) => {
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Ajouter une API REST' }).click();
  const form = page.locator('form');
  await form.getByLabel('Nom', { exact: true }).fill(API);
  await form.getByLabel('Préfixe des outils (définitif)').fill('asse');
  const picker = form.getByLabel('Fichier de description');

  // Unreadable: said, nothing offered.
  await picker.setInputFiles(file('cassé.yaml', 'openapi: [3.0\n'));
  await expect(form.getByText(/^Description YAML illisible \(ligne \d+\)/)).toBeVisible();
  await expect(form.getByText(/opération\(s\) proposée\(s\)/)).toHaveCount(0);
  // An alias holding itself, which no JSON can write: said too.
  await picker.setInputFiles(file('boucle.yaml', 'a: &x\n  b: *x\n'));
  await expect(
    form.getByText('Description YAML refusée : un alias qui se contient lui-même.'),
  ).toBeVisible();
  // An alias without its anchor, a merge of what is no table, aliases
  // repeated beyond a hundred: each said for what it is.
  await picker.setInputFiles(file('faute.yaml', 'a: *absente\n'));
  await expect(
    form.getByText('Description YAML illisible : un alias sans ancre définie avant lui.'),
  ).toBeVisible();
  await picker.setInputFiles(file('fusion.yaml', '<<: 1\n'));
  await expect(
    form.getByText(/^Description YAML illisible : une clé « << » qui ne fusionne pas une table/),
  ).toBeVisible();
  const repeated = (times: number) => Array.from({ length: times }, () => '*a').join(', ');
  // A hundred are read (the platform then finds no OpenAPI description).
  await picker.setInputFiles(file('cent.yaml', `a: &a [1]\nb: [${repeated(100)}]\n`));
  await expect(form.getByText(/^Description OpenAPI 3 attendue/)).toBeVisible();
  await picker.setInputFiles(file('alias.yaml', `a: &a [1]\nb: [${repeated(101)}]\n`));
  await expect(
    form.getByText(/^Description YAML refusée : des alias répétés plus de 100 fois/),
  ).toBeVisible();

  await picker.setInputFiles(file('assistance.yaml', DESCRIPTION));
  await expect(form.getByText(/^2 opération\(s\) proposée\(s\) ; 50 de plus/)).toBeVisible();
  await form.getByText('Remarques (1)').click();
  await expect(
    form.getByText('POST /sessions : laissée de côté, paramètre requis « X-Tenant »', {
      exact: false,
    }),
  ).toBeVisible();
  await form.getByRole('checkbox', { name: /GET \/tickets\/\{id\}/ }).check();
  await form.getByRole('button', { name: 'Ajouter les opérations cochées (1)' }).click();

  // Shown like any other operation, in place of the empty one; the address
  // the description gives filled in.
  const first = form.getByRole('group', { name: 'Opération 1' });
  await expect(first.getByLabel('Nom de l’opération')).toHaveValue('get_ticket');
  await expect(first.getByLabel('Intitulé')).toHaveValue('Lire un ticket');
  await expect(first.getByLabel(/^Chemin/)).toHaveValue('/tickets/{id}');
  await expect(form.getByRole('group', { name: 'Opération 2' })).toHaveCount(0);
  await expect(form.getByLabel(/^Adresse de l’API/)).toHaveValue(URL);

  // Read again: the operation already added cannot be checked twice.
  await picker.setInputFiles(file('assistance.yaml', DESCRIPTION));
  await expect(form.getByText(/^2 opération\(s\) proposée\(s\) ; 49 de plus/)).toBeVisible();
  await expect(form.getByRole('checkbox', { name: /GET \/tickets\/\{id\}/ })).toBeDisabled();
  await form.getByRole('checkbox', { name: /POST \/tickets / }).check();
  await form.getByRole('button', { name: 'Ajouter les opérations cochées (1)' }).click();
  const second = form.getByRole('group', { name: 'Opération 2' });
  await expect(second.getByLabel('Nom de l’opération')).toHaveValue('create_ticket');

  await form.getByRole('button', { name: 'Ajouter l’API' }).click();
  await expect(page.locator('form')).toHaveCount(0);
  const api = await stored(page);
  expect(api?.rest?.url).toBe(URL);
  expect(api?.rest?.operations.map((o) => o.name)).toEqual(['get_ticket', 'create_ticket']);
  expect(api?.rest?.operations[1]?.parameters[0]).toMatchObject({
    name: 'title',
    in: 'body',
    type: 'string',
    required: true,
  });
  await noError(page);
});

test('an API one operation short of fifty takes only one more', async ({ page }) => {
  await openAdmin(page, 'Connecteurs');
  const operations = Array.from({ length: 49 }, (_, i) => ({
    name: `op_${i}`,
    title: `Élément ${i}`,
    description: `Lit l’élément ${i}.`,
    method: 'GET' as const,
    path: `/items${i}`,
    parameters: [],
  }));
  const created = await page.request.post('/api/admin/connectors/rest', {
    headers: WRITE,
    data: { name: FULL, prefix: 'plei', url: URL, operations } satisfies NewRestConnector,
  });
  expect(created.status()).toBe(201);
  await page.goto('/admin');
  await page.getByRole('tab', { name: 'Connecteurs' }).click();
  await cardWith(page, FULL, 'Modifier')
    .getByRole('button', { name: 'Modifier', exact: true })
    .click();
  const form = page.locator('form');

  // A JSON description, sent as it is.
  const description = JSON.stringify({
    openapi: '3.1.0',
    paths: {
      '/tickets': {
        get: { operationId: 'listTickets', summary: 'Lister les tickets' },
        post: { operationId: 'createTicket', summary: 'Créer un ticket' },
      },
    },
  });
  await form.getByLabel('Fichier de description').setInputFiles({
    name: 'tickets.json',
    mimeType: 'application/json',
    buffer: Buffer.from(description),
  });
  await expect(form.getByText(/^2 opération\(s\) proposée\(s\) ; 1 de plus/)).toBeVisible();
  await form.getByRole('checkbox', { name: /GET \/tickets / }).check();
  // Fifty reached: the other one cannot be checked.
  await expect(form.getByRole('checkbox', { name: /POST \/tickets / })).toBeDisabled();
  // The form filled meanwhile: what was checked no longer fits, until the
  // operation typed is removed.
  await form.getByRole('button', { name: 'Ajouter une opération' }).click();
  const typed = form.getByRole('group', { name: 'Opération 50' });
  await typed.getByLabel('Nom de l’opération').fill('typed_here');
  await expect(
    form.getByRole('button', { name: 'Ajouter les opérations cochées (0)' }),
  ).toBeDisabled();
  // What no longer fits is shown unchecked.
  await expect(form.getByRole('checkbox', { name: /GET \/tickets / })).not.toBeChecked();
  await form.getByRole('button', { name: 'Retirer l’opération 50' }).click();
  await form.getByRole('button', { name: 'Ajouter les opérations cochées (1)' }).click();
  await expect(
    form.getByRole('group', { name: 'Opération 50' }).getByLabel('Nom de l’opération'),
  ).toHaveValue('list_tickets');
  await form.getByRole('button', { name: 'Enregistrer', exact: true }).click();
  await expect(page.locator('form')).toHaveCount(0);
  const api = await stored(page, FULL);
  expect(api?.rest?.operations).toHaveLength(50);
  expect(api?.rest?.operations[49]?.name).toBe('list_tickets');
  await noError(page);
});
