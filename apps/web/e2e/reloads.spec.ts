// While a screen reloads its list after a change, nothing can be changed from
// the former one: its controls are disabled until the list arrives, then the
// forms show what the API saved, and an open form what another administrator
// changed (ADR-041, ADR-042). Each test holds the list until it lets it
// through. The account it creates keeps no business role; the role and the
// MCP server it creates are deleted, even when it fails.
import type {
  AccountUpdate,
  ConnectorTest,
  Connectors,
  Directory,
  Roles,
  TestedTool,
} from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { expect, FOREIGN_HOST, noError, test } from './support/fixtures';
import { holdList } from './support/hold';

const ROLE = 'Réviseur e2e';
const SERVER = 'Outils e2e';
const WRITE = { 'X-Mimorya-Request': '1' };

test.afterEach(async ({ page }) => {
  const listed = await page.request.get('/api/admin/roles');
  if (listed.status() === 401) return;
  expect(listed.status()).toBe(200);
  const { roles } = (await listed.json()) as Roles;
  for (const role of roles.filter((r) => r.name === ROLE)) {
    const deleted = await page.request.delete(`/api/admin/roles/${role.id}`, { headers: WRITE });
    expect(deleted.status()).toBe(200);
  }
  const connectors = await page.request.get('/api/admin/connectors');
  expect(connectors.status()).toBe(200);
  const { instances } = (await connectors.json()) as Connectors;
  for (const connector of instances.filter((c) => c.name === SERVER)) {
    const deleted = await page.request.delete(`/api/admin/connectors/${connector.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
});

test('an account’s forms wait for, then follow, the reloaded directory', async ({ page }) => {
  const email = 'victor.chassagne@neo-maintenance.example';
  await openAdmin(page, 'Utilisateurs');
  await page.getByRole('button', { name: 'Ajouter un collaborateur' }).click();
  await page.getByLabel('Nom affiché').fill('Victor Chassagne');
  await page.getByLabel('Adresse e-mail').fill(email);
  await page.getByRole('button', { name: 'Créer le compte' }).click();
  await expect(page.getByText(`Invitation envoyée à ${email}`)).toBeVisible();

  const row = page.getByRole('row').filter({ hasText: email }).first();
  await row.getByRole('button', { name: 'Gérer' }).click();
  const directory = await holdList(page, '/api/admin/users');
  const jobTitle = page.getByLabel('Fonction');
  const saveProfile = page.getByRole('button', { name: 'Enregistrer', exact: true });
  const saveAttributes = page.getByRole('button', { name: 'Enregistrer les attributs' });

  let reload = directory.arm();
  await jobTitle.fill('Chef d’équipe');
  // Saved from the keyboard: the field gets the focus back after the lock.
  await jobTitle.press('Enter');
  await reload;
  // Until the directory arrives, nothing on the screen can be changed, nor
  // the account closed and reopened from the former directory.
  await expect(row.getByRole('button', { name: 'Fermer' })).toBeDisabled();
  await expect(jobTitle).toBeDisabled();
  await expect(saveProfile).toBeDisabled();
  await expect(saveAttributes).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Ajouter un collaborateur' })).toBeDisabled();
  // Disabled, the field has lost the focus to the page...
  await expect(jobTitle).not.toBeFocused();
  directory.release();
  await expect(saveProfile).toBeEnabled();
  // The row shows the account as the reloaded directory lists it.
  await expect(row).toContainText('Chef d’équipe');
  // ...and gets it back once the directory has arrived.
  await expect(jobTitle).toBeFocused();

  // Another administrator changes the account while its form is open...
  const listed = await page.request.get('/api/admin/users');
  const id = ((await listed.json()) as Directory).users.find((u) => u.email === email)?.id;
  const changed = await page.request.patch(`/api/admin/users/${id}`, {
    headers: WRITE,
    data: { jobTitle: 'Responsable de secteur' } satisfies AccountUpdate,
  });
  expect(changed.status()).toBe(200);
  // ...the next reload brings the change to the form, which saving would
  // otherwise overwrite with the former value.
  reload = directory.arm();
  await page.getByRole('button', { name: 'Ajouter un attribut' }).click();
  await page.getByLabel('Nom', { exact: true }).fill('department');
  await page.getByLabel('Valeur').fill('technique');
  await saveAttributes.click();
  await reload;
  await expect(jobTitle).toHaveValue('Chef d’équipe');
  await expect(saveAttributes).not.toBeFocused();
  // A focus given elsewhere meanwhile (a tab, outside the lock), even if then
  // left for the page, cancels giving it back.
  const tab = page.getByRole('tab', { name: 'Utilisateurs' });
  await tab.focus();
  await tab.blur();
  directory.release();
  await expect(jobTitle).toHaveValue('Responsable de secteur');
  await expect(saveAttributes).toBeEnabled();
  await expect(saveAttributes).not.toBeFocused();
  await expect
    .poll(async () => {
      const response = await page.request.get('/api/admin/users');
      const users = ((await response.json()) as Directory).users;
      return users.find((u) => u.email === email)?.attributes;
    })
    .toEqual({ department: 'technique' });
  await noError(page);
});

test('a role is edited again only from the reloaded list', async ({ page }) => {
  page.on('dialog', (dialog) => dialog.accept());
  await openAdmin(page, 'Rôles');
  await page.getByRole('button', { name: 'Nouveau rôle' }).click();
  await page.getByLabel('Nom').fill(ROLE);
  await page.getByLabel('Description').fill('Relit les plannings');
  await page.getByRole('checkbox', { name: 'Consulter les plannings' }).check();
  await page.getByRole('button', { name: 'Créer le rôle' }).click();
  const edit = page.getByRole('button', { name: `Modifier ${ROLE}` });
  await expect(edit).toBeEnabled();

  const roles = await holdList(page, '/api/admin/roles');
  // The change itself is held too: its form cannot be closed before its answer.
  let answer = () => {};
  const answered = new Promise<void>((done) => (answer = done));
  await page.route('**/api/admin/roles/*', async (route) => {
    if (route.request().method() === 'PATCH') await answered;
    await route.continue();
  });
  const reload = roles.arm();
  await edit.click();
  await page.getByLabel('Description').fill('Relit et commente les plannings');
  await page.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(page.getByRole('button', { name: 'Annuler' })).toBeDisabled();
  answer();
  await reload;
  // The card is back, as the former list shows it: it cannot be edited again
  // from there, nor can anything else on the screen be changed.
  await expect(page.getByText('Relit les plannings')).toBeVisible();
  await expect(edit).toBeDisabled();
  await expect(page.getByRole('button', { name: 'Nouveau rôle' })).toBeDisabled();
  // Shown during the lock, the card's button is dimmed past 150 ms all the same;
  // a button disabled on its own (a role still granted) is left as it was.
  await expect(edit).toHaveCSS('animation-name', 'locked-dim');
  await expect(edit).toHaveCSS('opacity', '0.5');
  const granted = page.getByRole('button', { name: 'Supprimer Responsable d’équipe' });
  await expect(granted).toHaveCSS('animation-name', 'none');
  roles.release();
  await expect(page.getByText('Relit et commente les plannings')).toBeVisible();
  await edit.click();
  await expect(page.getByLabel('Description')).toHaveValue('Relit et commente les plannings');
  await page.getByRole('button', { name: 'Annuler' }).click();
  await noError(page);

  await page.getByRole('button', { name: `Supprimer ${ROLE}` }).click();
  await expect(page.getByText(ROLE, { exact: true })).toHaveCount(0);
});

test('tool policies start again from each connection test', async ({ page }) => {
  const tool = (name: string, risk: TestedTool['risk']): TestedTool => ({
    name,
    title: name,
    description: 'Outil e2e',
    replacedDescription: null,
    enabled: true,
    risk,
    permissions: ['connector:use'],
  });
  // The server is never reached: each test gets the answer given here.
  const answers: ConnectorTest[] = [
    { ok: true, error: null, tools: [tool('mcp_oute_search', 'read')] },
    {
      ok: true,
      error: null,
      tools: [tool('mcp_oute_search', 'write'), tool('mcp_oute_delete', 'irreversible')],
    },
  ];
  await page.route('**/api/admin/connectors/*/test', (route) =>
    route.fulfill({ json: answers.shift() }),
  );
  await openAdmin(page, 'Connecteurs');
  await page.getByRole('button', { name: 'Ajouter un serveur MCP' }).click();
  await page.getByLabel('Nom').fill(SERVER);
  await page.getByLabel('Préfixe des outils (définitif)').fill('oute');
  await page.getByLabel(/^Adresse du serveur/).fill(`https://mcp.${FOREIGN_HOST}/mcp`);
  await page.getByRole('button', { name: 'Ajouter le serveur' }).click();

  const card = cardWith(page, SERVER, 'Tester la connexion');
  const testButton = card.getByRole('button', { name: 'Tester la connexion' });
  await expect(testButton).toBeEnabled();
  const connectors = await holdList(page, '/api/admin/connectors');
  const reload = connectors.arm();
  await testButton.click();
  await reload;
  // Disabled while testing, then by the reload's lock: it has lost the focus...
  await expect(testButton).toBeDisabled();
  await expect(testButton).not.toBeFocused();
  // The rules shown during the lock: their checkboxes, drawn by the interface,
  // are dimmed past 150 ms like the other controls.
  const enabled = card.getByRole('checkbox', { name: /mcp_oute_search/ });
  await expect(enabled).toBeDisabled();
  await expect(enabled).toHaveCSS('appearance', 'none');
  await expect(enabled).toHaveCSS('animation-name', 'locked-dim');
  await expect(enabled).toHaveCSS('opacity', '0.6');
  // Its mark (drawn next to it) dims with it, past 150 ms too.
  const mark = enabled.locator('xpath=following-sibling::*[1]');
  await expect(mark).toHaveCSS('animation-name', 'locked-dim');
  await expect(mark).toHaveCSS('opacity', '0.6');
  connectors.release();
  await expect(card.getByLabel('Risque de mcp_oute_search')).toHaveValue('read');
  // ...and gets it back once the list has arrived.
  await expect(testButton).toBeFocused();
  await testButton.click();
  // The second test's rules, not the first's: saving them keeps what it brought.
  await expect(card.getByLabel('Risque de mcp_oute_delete')).toHaveValue('irreversible');
  await expect(card.getByLabel('Risque de mcp_oute_search')).toHaveValue('write');
  await noError(page);
});
