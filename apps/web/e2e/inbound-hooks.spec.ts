// A person's inbound hooks (ADR-109, ADR-110): Léa makes one and reads its
// token, shown once. Another system then posts to its address, as it would:
// without a session, with the token alone. She reads what became of the
// delivery and opens its conversation, where her request and the data
// received are apart (the data as the model read it, the invisible character
// it held taken out and the platform saying so, ADR-158), then the agent's
// answer. She pauses it (a delivery
// answers as to an unknown hook), resumes it, renews its token twice (the
// former no longer serves; what the notice said of a copy goes with it, and
// a copy the browser refuses is said), changes it and deletes it, each
// change read back from the API. Her test hooks are deleted, and their
// conversations archived, whatever happens. The rules themselves are tested
// on the API (`crates/api/tests/inbound_hook*.rs`).
import type { APIRequestContext, Page } from '@playwright/test';
import type { ConversationList, InboundHook, InboundHookList } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';

const WRITE = { 'X-Mimorya-Request': '1' };
/** What every hook of the test is titled with, for its cleanup. */
const MARK = ' (e2e)';
const TITLE = `Alerte supervision${MARK}`;
const INSTRUCTION = 'Résume l’alerte reçue et propose une intervention.';
const POSTED = '{"alerte": "Pompe P2 à l’arrêt", "site": "Lyon Nord"}';
/** The same, with a zero-width space the platform takes out (ADR-158). */
const POSTED_HIDING = POSTED.replace('Lyon', 'Lyon\u200b');
/** What the end-to-end model answers what a hook sends (`e2e_server/hooks.rs`). */
const ANSWER = 'Panne reçue de la supervision';

/** The page the test signed in, for its cleanup. */
const left: { page?: Page } = {};

test.afterEach(async () => {
  const page = left.page;
  left.page = undefined;
  if (!page) return;
  for (const hook of await hooks(page)) {
    if (!hook.title.endsWith(MARK)) continue;
    const deleted = await page.request.delete(`/api/inbound-hooks/${hook.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
  // The application deletes no conversation: those the test opened are put away.
  const listed = await page.request.get('/api/conversations');
  expect(listed.status()).toBe(200);
  for (const conversation of ((await listed.json()) as ConversationList).conversations) {
    if (!conversation.title.endsWith(MARK)) continue;
    const archived = await page.request.patch(`/api/conversations/${conversation.id}`, {
      headers: WRITE,
      data: { archived: true },
    });
    expect(archived.status()).toBe(200);
  }
});

/** The hooks of whom `page` signed in, as the API records them. */
async function hooks(page: Page): Promise<InboundHook[]> {
  const listed = await page.request.get('/api/inbound-hooks');
  expect(listed.status()).toBe(200);
  return ((await listed.json()) as InboundHookList).hooks;
}

async function hook(page: Page, title: string): Promise<InboundHook | undefined> {
  return (await hooks(page)).find((h) => h.title === title);
}

/** What another system posting `body` to `url` with `token` gets: no session, no header. */
async function deliver(outside: APIRequestContext, url: string, token: string, body = POSTED) {
  return outside.post(url, {
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    data: body,
  });
}

test('a person makes a hook, another system posts to it, and she reads the run', async ({
  page,
  request,
}) => {
  page.on('dialog', (dialog) => dialog.accept());
  await page.context().grantPermissions(['clipboard-read', 'clipboard-write']);
  await signIn(page, demo.lea);
  left.page = page;
  await page.getByRole('link', { name: 'Déclencheurs' }).click();
  await expect(page.getByRole('heading', { name: 'Déclencheurs' })).toBeVisible();

  const form = page.getByRole('region', { name: 'Nouveau déclencheur' });
  await form.getByLabel('Intitulé').fill(TITLE);
  await form.getByLabel('Demande à l’agent').fill(INSTRUCTION);
  await form.getByRole('button', { name: 'Créer' }).click();

  // The token, shown this once, read first.
  const notice = page.getByRole('region', { name: `Jeton de « ${TITLE} »` });
  await expect(notice.getByRole('heading')).toBeFocused();
  const token = await notice.getByLabel(/^Jeton, à envoyer/).inputValue();
  expect(token).toMatch(/^mhk_/);
  await notice.getByRole('button', { name: 'Copier le jeton' }).click();
  await expect(notice.getByRole('button', { name: 'Jeton copié' })).toBeVisible();
  expect(await page.evaluate(() => navigator.clipboard.readText())).toBe(token);
  await expect(form.getByLabel('Intitulé')).toHaveValue('');
  const made = await hook(page, TITLE);
  expect(made).toMatchObject({ instruction: INSTRUCTION, enabled: true, lastDelivery: null });
  const url = made?.url ?? '';
  expect(url).toBe(`http://localhost:3000/api/hooks/${made?.id}`);
  await expect(notice.getByText(url, { exact: true })).toBeVisible();
  await notice.getByRole('button', { name: 'J’ai noté le jeton' }).click();
  await expect(notice).toHaveCount(0);
  const card = page.getByRole('article', { name: TITLE });
  await expect(card.getByText('Actif', { exact: true })).toBeVisible();
  await expect(card.getByText('Aucun envoi reçu pour l’instant.')).toBeVisible();
  // Gone with the page: the list never holds it.
  expect(JSON.stringify(await hooks(page))).not.toContain(token);

  // Another system posts, its token its only proof.
  const accepted = await deliver(request, url, token, POSTED_HIDING);
  expect(accepted.status()).toBe(202);
  await expect.poll(async () => (await hook(page, TITLE))?.lastDelivery?.outcome).toBe('started');
  await page.reload();
  await expect(card.getByText('Lancé', { exact: true })).toBeVisible();
  await card.getByRole('link', { name: 'Ouvrir la conversation du dernier envoi lancé' }).click();
  await page.waitForURL('**/chat/**');
  await expect(page.getByText('Envoyé par un déclencheur')).toBeVisible();
  await expect(page.getByText(INSTRUCTION, { exact: true })).toBeVisible();
  await page.getByText('Données reçues').click();
  // As the model read it, and what the platform took out of it.
  await expect(page.getByText(POSTED, { exact: true })).toBeVisible();
  await expect(
    page.getByText('[1 caractère invisible retiré par la plateforme]', { exact: true }),
  ).toBeVisible();
  await expect(page.getByText(ANSWER)).toBeVisible();
  await noError(page);

  // Paused: a delivery answers as to an unknown hook.
  await page.goto('/inbound-hooks');
  await card.getByRole('button', { name: `Mettre en pause « ${TITLE} »` }).click();
  await expect(card.getByText('En pause', { exact: true })).toBeVisible();
  expect((await hook(page, TITLE))?.enabled).toBe(false);
  const paused = await deliver(request, url, token);
  expect(paused.status()).toBe(404);
  await card.getByRole('button', { name: `Reprendre « ${TITLE} »` }).click();
  await expect(card.getByText('Actif', { exact: true })).toBeVisible();
  expect((await hook(page, TITLE))?.enabled).toBe(true);

  // A new token: the former no longer serves.
  await card.getByRole('button', { name: `Nouveau jeton pour « ${TITLE} »` }).click();
  const renewed = page.getByRole('region', { name: `Jeton de « ${TITLE} »` });
  const field = renewed.getByLabel(/^Jeton, à envoyer/);
  const newToken = await field.inputValue();
  expect(newToken).toMatch(/^mhk_/);
  expect(newToken).not.toBe(token);
  expect((await deliver(request, url, token)).status()).toBe(404);
  // Copied, then renewed again while shown: the notice says nothing of the
  // former's copy.
  await renewed.getByRole('button', { name: 'Copier le jeton' }).click();
  await expect(renewed.getByRole('button', { name: 'Jeton copié' })).toBeVisible();
  await card.getByRole('button', { name: `Nouveau jeton pour « ${TITLE} »` }).click();
  await expect(field).not.toHaveValue(newToken);
  await expect(renewed.getByRole('button', { name: 'Copier le jeton' })).toBeVisible();
  expect((await deliver(request, url, newToken)).status()).toBe(404);
  // A copy the browser refuses says so.
  await page.evaluate(() => {
    navigator.clipboard.writeText = () => Promise.reject(new Error('refusée'));
  });
  await renewed.getByRole('button', { name: 'Copier le jeton' }).click();
  await expect(renewed.getByRole('status')).toHaveText(/^Copie impossible/);
  await renewed.getByRole('button', { name: 'J’ai noté le jeton' }).click();

  // Changed, then deleted.
  const changed = 'Résume l’alerte et dis qui prévenir.';
  await card.getByRole('button', { name: `Modifier « ${TITLE} »` }).click();
  await card.getByLabel('Demande à l’agent').fill(changed);
  await card.getByRole('button', { name: 'Enregistrer' }).click();
  await expect(card.getByRole('button', { name: `Modifier « ${TITLE} »` })).toBeFocused();
  await expect(card.getByText(changed)).toBeVisible();
  expect((await hook(page, TITLE))?.instruction).toBe(changed);
  await card.getByRole('button', { name: `Supprimer « ${TITLE} »` }).click();
  await expect(page.getByRole('article', { name: TITLE })).toHaveCount(0);
  expect(await hook(page, TITLE)).toBeUndefined();
  await noError(page);
});
