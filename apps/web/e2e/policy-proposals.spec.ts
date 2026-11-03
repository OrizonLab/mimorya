// An allow policy takes two administrators (ADR-168, ADR-169). Claire, a
// second administrator of the policies made for each test, decides what
// Paul proposes from « Politiques d’accès »:
// - she approves a new authorisation, listed as proposed; Paul proposes a
//   change of it, which she approves on the same policy; Paul switches it
//   off, then deletes it;
// - she refuses another proposal, which opens nothing; a change Paul
//   proposes is outdated by his switch-off, which she cannot approve, and
//   he withdraws it;
// - she switches an authorisation off while Paul's change of it is open:
//   with nothing reloading his page, the change is refused, and cancelling
//   opens the form again on the policy as it stands; on another one, once
//   his lists reload, his form says it changed and the change is refused.
// Whatever happens, a test leaves no allow policy of its own nor proposal
// waiting, and Claire's account without role, disabled. The rules
// themselves are tested on the API (`crates/api/tests/policy_proposal*.rs`).
import type { Page } from '@playwright/test';
import type {
  AccountUpdate,
  Directory,
  NewAccount,
  NewRole,
  Policies,
  PolicyProposals,
  PolicyProposalView,
  RoleGrant,
} from '../lib/api/contract';
import { cardWith, openAdmin } from './support/admin';
import { expect, noError, test } from './support/fixtures';
import { lastLink } from './support/mail';

const WRITE = { 'X-Mimorya-Request': '1' };
const MARK = '(e2e double validation)';
const CHOSEN = 'phrase-de-passe-politiques-e2e';

/** What a test made, for its cleanup. */
const made: { paul?: Page; claire?: string; role?: string } = {};

test.afterEach(async () => {
  const paul = made.paul;
  if (!paul) return;
  try {
    // Its allow policies deleted, their proposals with them; Paul withdraws
    // his other waiting proposals and refuses Claire's.
    for (const policy of await grantsOf(paul)) {
      const deleted = await paul.request.delete(`/api/admin/policies/${policy.id}`, {
        headers: WRITE,
      });
      expect.soft(deleted.status()).toBe(200);
    }
    for (const proposal of (await proposalsOf(paul)).filter((p) => p.status === 'pending')) {
      const decision = proposal.mine ? 'withdrawal' : 'rejection';
      const decided = await paul.request.post(
        `/api/admin/policy-proposals/${proposal.id}/${decision}`,
        { headers: WRITE },
      );
      expect.soft(decided.status()).toBe(200);
    }
  } finally {
    // Whatever happened above, Claire keeps no role and no active account.
    if (made.claire) {
      const directory = (await (await paul.request.get('/api/admin/users')).json()) as Directory;
      const claire = directory.users.find((user) => user.id === made.claire);
      for (const assignment of claire?.assignments ?? []) {
        const revoked = await paul.request.delete(`/api/admin/assignments/${assignment.id}`, {
          headers: WRITE,
        });
        expect.soft(revoked.status()).toBe(200);
      }
      const disabled = await paul.request.patch(`/api/admin/users/${made.claire}`, {
        headers: WRITE,
        data: { status: 'disabled' } satisfies AccountUpdate,
      });
      expect.soft(disabled.status()).toBe(200);
    }
    if (made.role) {
      const removed = await paul.request.delete(`/api/admin/roles/${made.role}`, {
        headers: WRITE,
      });
      expect.soft(removed.status()).toBe(200);
    }
    made.paul = made.claire = made.role = undefined;
  }
});

/** The tests' proposals, as the API lists them. */
async function proposalsOf(page: Page): Promise<PolicyProposalView[]> {
  const listed = (await (
    await page.request.get('/api/admin/policy-proposals')
  ).json()) as PolicyProposals;
  return listed.proposals.filter((p) => p.description.includes(MARK));
}

/**
 * Approves `description` as `page`, then waits for the decision itself:
 * decided, the proposal no longer offers its approval. Before, the list
 * holding the proposal and any policy card already matches `cardWith`.
 */
async function approve(page: Page, description: string) {
  const button = page.getByRole('button', { name: `Approuver « ${description} »` });
  await button.click();
  await expect(button).toHaveCount(0);
}

/** The tests' allow policies, as the API lists them. */
async function grantsOf(page: Page) {
  const listing = (await (await page.request.get('/api/admin/policies')).json()) as Policies;
  return listing.policies.filter((p) => p.effect === 'allow' && p.description.includes(MARK));
}

/**
 * Signs Paul in on `paul`, then makes Claire: an account holding only the
 * administration of the policies, company-wide, activated on `claire` from
 * her invitation. Both pages accept the confirmations.
 */
async function administrators(paul: Page, claire: Page) {
  paul.on('dialog', (dialog) => dialog.accept());
  claire.on('dialog', (dialog) => dialog.accept());
  await openAdmin(paul, 'Politiques d’accès');
  made.paul = paul;
  const stamp = Date.now();
  const email = `claire.politiques.${stamp}@neo-maintenance.example`;
  const role = await paul.request.post('/api/admin/roles', {
    headers: WRITE,
    data: {
      name: `Politiques d’accès ${stamp} ${MARK}`,
      description: 'Administre les politiques d’accès, pour le test',
      permissions: ['policy:manage'],
    } satisfies NewRole,
  });
  expect(role.status()).toBe(201);
  made.role = ((await role.json()) as { id: string }).id;
  const account = await paul.request.post('/api/admin/users', {
    headers: WRITE,
    data: { email, displayName: 'Claire Dumas' } satisfies NewAccount,
  });
  expect(account.status()).toBe(201);
  made.claire = ((await account.json()) as { id: string }).id;
  const granted = await paul.request.post(`/api/admin/users/${made.claire}/assignments`, {
    headers: WRITE,
    data: { roleId: made.role, scope: { type: 'tenant' } } satisfies RoleGrant,
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

/** Proposes, from the open tab, an authorisation on interventions in `status`. */
async function propose(page: Page, description: string, status: string) {
  await page.getByRole('button', { name: 'Proposer une autorisation' }).click();
  await page.getByLabel('Description (ce que la politique autorise, et à qui)').fill(description);
  await page.getByRole('checkbox', { name: /^Consulter les interventions/ }).check();
  await page.getByLabel('Attribut').fill('resource.attributes.status');
  await page.getByLabel('Valeur').fill(status);
  await page.getByRole('button', { name: 'Proposer', exact: true }).click();
  await waiting(page, description);
}

/** Proposes, from the open tab, the change of `policy`'s description to `description`. */
async function proposeChange(page: Page, policy: string, description: string) {
  await page.getByRole('button', { name: `Proposer une modification de ${policy}` }).click();
  await page.getByLabel('Description (ce que la politique autorise, et à qui)').fill(description);
  await page.getByRole('button', { name: 'Proposer', exact: true }).click();
  await waiting(page, description);
}

/** The status badge of the proposal `description`, in its card's header row. */
function shownAs(page: Page, description: string, status: string) {
  return page
    .getByText(description, { exact: true })
    .locator('..')
    .getByText(status, { exact: true });
}

/** The author's proposal `description` is shown waiting, with no approval for them. */
async function waiting(page: Page, description: string) {
  const card = cardWith(page, description, `Retirer « ${description} »`);
  await expect(card.getByText('En attente', { exact: true })).toBeVisible();
  await expect(card.getByRole('button', { name: /^Approuver/ })).toHaveCount(0);
}

test('an authorisation is approved by another administrator, then changed, switched off and deleted', async ({
  page,
  newContext,
}) => {
  const granted = `Interventions terminées ouvertes à tous ${MARK}`;
  const renamed = `Interventions terminées consultables partout ${MARK}`;
  const paul = page;
  const claire = await (await newContext()).newPage();
  await administrators(paul, claire);

  // Paul proposes: nothing is written yet.
  await openPolicies(paul);
  await propose(paul, granted, 'completed');
  const [proposed] = await proposalsOf(paul);
  expect(proposed?.status).toBe('pending');
  expect(proposed?.permissions).toEqual(['intervention:read']);
  expect(await grantsOf(paul)).toEqual([]);

  // Claire approves it: the policy is written, as proposed.
  await openPolicies(claire);
  await approve(claire, granted);
  await expect(
    cardWith(claire, granted, 'Désactiver').getByText('Autorisation', { exact: true }),
  ).toBeVisible();
  await noError(claire);
  expect((await proposalsOf(claire))[0]?.decidedBy?.displayName).toBe('Claire Dumas');
  const [grant] = await grantsOf(paul);
  expect(grant?.enabled).toBe(true);
  expect(grant?.condition).toEqual({
    all: [
      { op: 'eq', left: { attr: 'resource.attributes.status' }, right: { value: 'completed' } },
    ],
  });

  // Paul proposes a change of it; approved, it changes that same policy.
  await openPolicies(paul);
  await proposeChange(paul, granted, renamed);
  const change = (await proposalsOf(paul)).find((p) => p.description === renamed);
  expect(change?.policyId).toBe(grant?.id);
  await expect(paul.getByText(`Modifie l’autorisation « ${granted} »`)).toBeVisible();
  await openPolicies(claire);
  await approve(claire, renamed);
  await expect(cardWith(claire, renamed, 'Désactiver')).toBeVisible();
  const changed = await grantsOf(paul);
  expect(changed.map((p) => [p.id, p.description])).toEqual([[grant?.id, renamed]]);

  // Paul switches it off, then deletes it, both at once.
  await openPolicies(paul);
  await cardWith(paul, renamed, 'Désactiver')
    .getByRole('button', { name: 'Désactiver', exact: true })
    .click();
  await expect(
    paul.getByText(
      'Une autorisation se réactive par une proposition qu’un autre administrateur approuve.',
    ),
  ).toBeVisible();
  expect((await grantsOf(paul))[0]?.enabled).toBe(false);
  await paul.getByRole('button', { name: `Supprimer ${renamed}` }).click();
  await expect(paul.getByRole('button', { name: `Supprimer ${renamed}` })).toHaveCount(0);
  await noError(paul);
  expect(await grantsOf(paul)).toEqual([]);
});

test('a proposal is refused, or outdated by a switch-off and withdrawn', async ({
  page,
  newContext,
}) => {
  const refused = `Interventions annulées ouvertes à tous ${MARK}`;
  const granted = `Interventions terminées pour la facturation ${MARK}`;
  const outdated = `Interventions terminées pour tous les services ${MARK}`;
  const paul = page;
  const claire = await (await newContext()).newPage();
  await administrators(paul, claire);

  // Refused by Claire, a proposal opens nothing.
  await openPolicies(paul);
  await propose(paul, refused, 'cancelled');
  await openPolicies(claire);
  await claire.getByRole('button', { name: `Refuser « ${refused} »` }).click();
  await expect(shownAs(claire, refused, 'Refusée')).toBeVisible();
  expect((await proposalsOf(paul)).find((p) => p.description === refused)?.status).toBe('rejected');
  expect(await grantsOf(paul)).toEqual([]);

  // An authorisation approved, Paul proposes a change, then switches it off:
  // the change is outdated, and Claire cannot approve it.
  await openPolicies(paul);
  await propose(paul, granted, 'completed');
  await openPolicies(claire);
  await approve(claire, granted);
  await expect(cardWith(claire, granted, 'Désactiver')).toBeVisible();
  await openPolicies(paul);
  await proposeChange(paul, granted, outdated);
  await cardWith(paul, granted, 'Désactiver')
    .getByRole('button', { name: 'Désactiver', exact: true })
    .click();
  await expect(
    cardWith(paul, outdated, `Retirer « ${outdated} »`).getByText('Périmée', { exact: true }),
  ).toBeVisible();
  await openPolicies(claire);
  await expect(claire.getByRole('button', { name: `Approuver « ${outdated} »` })).toBeDisabled();
  expect((await proposalsOf(claire)).find((p) => p.description === outdated)?.outdated).toBe(true);

  // Paul withdraws it.
  await openPolicies(paul);
  await paul.getByRole('button', { name: `Retirer « ${outdated} »` }).click();
  await expect(shownAs(paul, outdated, 'Retirée')).toBeVisible();
  await noError(paul);
  expect((await proposalsOf(paul)).find((p) => p.description === outdated)?.status).toBe(
    'withdrawn',
  );
});

test('a change is refused on an authorisation switched off since its form opened', async ({
  page,
  newContext,
}) => {
  const first = `Interventions terminées pour les responsables ${MARK}`;
  const second = `Interventions terminées pour la direction ${MARK}`;
  const other = `Interventions planifiées ouvertes à tous ${MARK}`;
  const renamed = `Interventions terminées pour toute l’entreprise ${MARK}`;
  const paul = page;
  const claire = await (await newContext()).newPage();
  await administrators(paul, claire);
  await openPolicies(paul);
  await propose(paul, first, 'completed');
  await propose(paul, second, 'completed');
  await openPolicies(claire);
  for (const description of [first, second]) {
    await approve(claire, description);
    await expect(cardWith(claire, description, 'Désactiver')).toBeVisible();
  }
  const changeForm = (description: string) =>
    paul.getByRole('button', { name: `Proposer une modification de ${description}` });
  const enabledOnce = paul.getByRole('checkbox', { name: 'Active une fois approuvée' });
  /** Claire switches `description` off. */
  async function switchOff(description: string) {
    await cardWith(claire, description, 'Désactiver')
      .getByRole('button', { name: 'Désactiver', exact: true })
      .click();
    await expect(
      cardWith(claire, description, `Supprimer ${description}`).getByText('Désactivée', {
        exact: true,
      }),
    ).toBeVisible();
  }

  // Paul opens a change of the first, still enabled; Claire switches it off
  // meanwhile; nothing reloads Paul's page before he proposes.
  await openPolicies(paul);
  await propose(paul, other, 'planned');
  await changeForm(first).click();
  await expect(enabledOnce).toBeChecked();
  // Nothing changed yet: no warning.
  await expect(paul.getByText(/a changé depuis l’ouverture du formulaire/)).toHaveCount(0);
  await switchOff(first);
  await paul.getByRole('button', { name: 'Proposer', exact: true }).click();
  await expect(paul.getByText(/a changé depuis que vous l’avez ouverte/)).toBeVisible();
  // Cancelled after the refusal, the lists reload: the form opens again on
  // the policy as it stands, switched off.
  await paul.getByRole('button', { name: 'Annuler' }).click();
  await changeForm(first).click();
  await expect(enabledOnce).not.toBeChecked();
  await paul.getByRole('button', { name: 'Annuler' }).click();

  // Paul opens a change of the second; Claire switches it off; Paul's lists
  // reload (he withdraws another proposal): his form, frozen on what he saw,
  // says so and is still refused.
  await changeForm(second).click();
  await paul.getByLabel('Description (ce que la politique autorise, et à qui)').fill(renamed);
  await switchOff(second);
  await paul.getByRole('button', { name: `Retirer « ${other} »` }).click();
  await expect(shownAs(paul, other, 'Retirée')).toBeVisible();
  await expect(paul.getByText(/a changé depuis l’ouverture du formulaire/)).toBeVisible();
  await paul.getByRole('button', { name: 'Proposer', exact: true }).click();
  await expect(paul.getByText(/a changé depuis que vous l’avez ouverte/)).toBeVisible();
  expect((await proposalsOf(paul)).map((p) => p.description)).not.toContain(renamed);
  const states = (await grantsOf(paul)).map((p) => [p.description, p.enabled]).sort();
  expect(states).toEqual(
    [
      [first, false],
      [second, false],
    ].sort(),
  );
});
