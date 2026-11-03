// An allow policy takes two administrators (ADR-168): Paul proposes one from
// « Politiques d’accès »; Claire, a second administrator of the policies
// made for the test, approves it, and it is listed as an authorisation.
// Paul proposes another, which Claire refuses. Paul switches the first one
// off, then deletes it. Whatever happens, the test leaves no allow policy of
// its own nor proposal waiting, and Claire's account without role, disabled.
// The rules themselves are tested on the API
// (`crates/api/tests/policy_proposal*.rs`).
import type { Page } from '@playwright/test';
import type { Directory, Policies, PolicyProposals, PolicyProposalView } from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';
import { lastLink } from './support/mail';

const WRITE = { 'X-Mimorya-Request': '1' };
const MARK = '(e2e double validation)';
const CHOSEN = 'phrase-de-passe-politiques-e2e';
const GRANTED = `Interventions terminées ouvertes à tous ${MARK}`;
const REFUSED = `Interventions annulées ouvertes à tous ${MARK}`;

/** What the test made, for its cleanup. */
const made: { paul?: Page; claire?: string; role?: string } = {};

test.afterEach(async () => {
  const paul = made.paul;
  if (!paul) return;
  // Its allow policies deleted, their proposals with them.
  const listing = (await (await paul.request.get('/api/admin/policies')).json()) as Policies;
  for (const policy of listing.policies.filter((p) => p.description.includes(MARK))) {
    const deleted = await paul.request.delete(`/api/admin/policies/${policy.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
  // Paul withdraws his own waiting proposals and refuses Claire's.
  for (const proposal of (await proposalsOf(paul)).filter((p) => p.status === 'pending')) {
    const decision = proposal.mine ? 'withdrawal' : 'rejection';
    const decided = await paul.request.post(
      `/api/admin/policy-proposals/${proposal.id}/${decision}`,
      { headers: WRITE },
    );
    expect(decided.status()).toBe(200);
  }
  if (made.claire) {
    const directory = (await (await paul.request.get('/api/admin/users')).json()) as Directory;
    const claire = directory.users.find((user) => user.id === made.claire);
    for (const assignment of claire?.assignments ?? []) {
      const revoked = await paul.request.delete(`/api/admin/assignments/${assignment.id}`, {
        headers: WRITE,
      });
      expect(revoked.status()).toBe(200);
    }
    const disabled = await paul.request.patch(`/api/admin/users/${made.claire}`, {
      headers: WRITE,
      data: { status: 'disabled' },
    });
    expect(disabled.status()).toBe(200);
  }
  if (made.role) {
    const removed = await paul.request.delete(`/api/admin/roles/${made.role}`, { headers: WRITE });
    expect(removed.status()).toBe(200);
  }
  made.paul = made.claire = made.role = undefined;
});

/** The test's proposals, as the API lists them. */
async function proposalsOf(page: Page): Promise<PolicyProposalView[]> {
  const listed = (await (
    await page.request.get('/api/admin/policy-proposals')
  ).json()) as PolicyProposals;
  return listed.proposals.filter((p) => p.description.includes(MARK));
}

/** The test's allow policies, as the API lists them. */
async function grantsOf(page: Page) {
  const listing = (await (await page.request.get('/api/admin/policies')).json()) as Policies;
  return listing.policies.filter((p) => p.effect === 'allow' && p.description.includes(MARK));
}

/**
 * Claire, an account holding only the administration of the policies,
 * company-wide, made by Paul and activated from her invitation.
 */
async function secondAdministrator(paul: Page, claire: Page) {
  const stamp = Date.now();
  const email = `claire.politiques.${stamp}@neo-maintenance.example`;
  const role = await paul.request.post('/api/admin/roles', {
    headers: WRITE,
    data: {
      name: `Politiques d’accès ${stamp} ${MARK}`,
      description: 'Administre les politiques d’accès, pour le test',
      permissions: ['policy:manage'],
    },
  });
  expect(role.status()).toBe(201);
  made.role = ((await role.json()) as { id: string }).id;
  const account = await paul.request.post('/api/admin/users', {
    headers: WRITE,
    data: { email, displayName: 'Claire Dumas' },
  });
  expect(account.status()).toBe(201);
  made.claire = ((await account.json()) as { id: string }).id;
  const granted = await paul.request.post(`/api/admin/users/${made.claire}/assignments`, {
    headers: WRITE,
    data: { roleId: made.role, scope: { type: 'tenant' } },
  });
  expect(granted.status()).toBe(201);

  const { link } = await lastLink(email, 'invitation');
  await claire.goto(link);
  await claire.getByLabel('Mot de passe (12 caractères minimum)').fill(CHOSEN);
  await claire.getByLabel('Confirmation').fill(CHOSEN);
  await claire.getByRole('button', { name: 'Activer mon compte' }).click();
  await claire.waitForURL('**/chat');
}

/** Opens « Politiques d’accès » afresh in a signed-in page. */
async function openPolicies(page: Page) {
  await page.goto('/admin');
  await page.getByRole('tab', { name: 'Politiques d’accès' }).click();
  await expect(page.getByText('Chargement…')).toHaveCount(0);
}

/** Proposes, from the open tab, an allow policy on completed or cancelled interventions. */
async function propose(page: Page, description: string, status: string) {
  await page.getByRole('button', { name: 'Proposer une autorisation' }).click();
  await page.getByLabel('Description (ce que la politique autorise, et à qui)').fill(description);
  await page.getByRole('checkbox', { name: /^Consulter les interventions/ }).check();
  await page.getByLabel('Attribut').fill('resource.attributes.status');
  await page.getByLabel('Valeur').fill(status);
  await page.getByRole('button', { name: 'Proposer', exact: true }).click();
  await expect(
    cardWith(page, description, 'Retirer').getByText('En attente', { exact: true }),
  ).toBeVisible();
}

test('an allow policy is proposed by one administrator and approved by another', async ({
  page,
  newContext,
}) => {
  const paul = page;
  paul.on('dialog', (dialog) => dialog.accept());
  await openAdmin(paul, 'Politiques d’accès');
  made.paul = paul;
  const claire = await (await newContext()).newPage();
  claire.on('dialog', (dialog) => dialog.accept());
  await secondAdministrator(paul, claire);

  // Paul proposes: nothing is written yet, and he cannot approve it.
  await openPolicies(paul);
  await propose(paul, GRANTED, 'completed');
  await expect(
    cardWith(paul, GRANTED, 'Retirer').getByRole('button', { name: 'Approuver' }),
  ).toHaveCount(0);
  const [proposed] = await proposalsOf(paul);
  expect(proposed?.status).toBe('pending');
  expect(proposed?.permissions).toEqual(['intervention:read']);
  expect(await grantsOf(paul)).toEqual([]);

  // Claire approves it: the policy is written, as proposed.
  await openPolicies(claire);
  await cardWith(claire, GRANTED, 'Approuver')
    .getByRole('button', { name: 'Approuver', exact: true })
    .click();
  await expect(
    cardWith(claire, GRANTED, 'Désactiver').getByText('Autorisation', { exact: true }),
  ).toBeVisible();
  await noError(claire);
  const [approved] = await proposalsOf(claire);
  expect(approved?.status).toBe('approved');
  expect(approved?.decidedBy?.displayName).toBe('Claire Dumas');
  const [grant] = await grantsOf(paul);
  expect(grant?.enabled).toBe(true);
  expect(grant?.condition).toEqual({
    all: [
      { op: 'eq', left: { attr: 'resource.attributes.status' }, right: { value: 'completed' } },
    ],
  });

  // Another proposal of Paul's, refused by Claire, opens nothing.
  await openPolicies(paul);
  await propose(paul, REFUSED, 'cancelled');
  await openPolicies(claire);
  await cardWith(claire, REFUSED, 'Refuser')
    .getByRole('button', { name: 'Refuser', exact: true })
    .click();
  await expect(claire.getByText('Refusée', { exact: true })).toBeVisible();
  const refused = (await proposalsOf(paul)).find((p) => p.description === REFUSED);
  expect(refused?.status).toBe('rejected');
  expect((await grantsOf(paul)).map((p) => p.description)).toEqual([GRANTED]);

  // Paul switches the authorisation off, then deletes it, both at once.
  await openPolicies(paul);
  await cardWith(paul, GRANTED, 'Désactiver')
    .getByRole('button', { name: 'Désactiver', exact: true })
    .click();
  await expect(
    paul.getByText(
      'Une autorisation se réactive par une proposition qu’un autre administrateur approuve.',
    ),
  ).toBeVisible();
  expect((await grantsOf(paul))[0]?.enabled).toBe(false);
  await paul.getByRole('button', { name: `Supprimer ${GRANTED}` }).click();
  await expect(paul.getByRole('button', { name: `Supprimer ${GRANTED}` })).toHaveCount(0);
  await noError(paul);
  expect(await grantsOf(paul)).toEqual([]);
});
