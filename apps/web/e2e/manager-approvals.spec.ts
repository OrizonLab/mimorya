// Validation by a manager, through the browser (ADR-059). The company has its
// managers validate the actions that write: Nadia's go to Marc. She sees who
// decides, with no button to approve, and may withdraw her request; Marc sees
// for whom the action is, decides it, and is not led to her run. The routing
// itself is tested on the API (`crates/api/tests/manager_approvals.rs`).
import type { BrowserContext, Page } from '@playwright/test';
import type { Approval, ApprovalList, ApprovalRoutingUpdate, ToolRisk } from '../lib/api/contract';
import { refuseWaiting } from './support/approvals';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { REFERENCE_REQUEST } from './support/scenario';

const WRITE = { 'X-Mimorya-Request': '1' };
const REFUSED = 'Le planning est inchangé : la validation a été refusée.';

/** The administrator and the manager of the test, for its cleanup. */
const people: { paul?: Page; marc?: Page } = {};

async function routeToManagers(paul: Page, risks: ToolRisk[]) {
  const set = await paul.request.put('/api/admin/approval-routing', {
    headers: WRITE,
    data: { managerRisks: risks } satisfies ApprovalRoutingUpdate,
  });
  expect(set.status()).toBe(200);
}

/** Has Paul send Nadia's write actions to Marc, then Nadia ask the agent. */
async function nadiaAsks(page: Page, newContext: () => Promise<BrowserContext>) {
  people.paul = await (await newContext()).newPage();
  people.marc = await (await newContext()).newPage();
  await signIn(people.paul, demo.paul);
  await signIn(people.marc, demo.marc);
  await routeToManagers(people.paul, ['write']);
  await signIn(page, demo.nadia);
  await page.getByLabel('Votre message').fill(REFERENCE_REQUEST);
  await page.getByRole('button', { name: 'Envoyer' }).click();
  await expect(page.getByText('En attente de la validation de Marc Lefèvre.')).toBeVisible({
    timeout: 30_000,
  });
  await expect(page.getByRole('button', { name: 'Valider' })).toHaveCount(0);
}

test.afterEach(async () => {
  // Marc refuses what a failed test left waiting; nothing goes to him after.
  if (people.marc) await refuseWaiting(people.marc);
  if (people.paul) await routeToManagers(people.paul, []);
  people.paul = undefined;
  people.marc = undefined;
});

test('the manager decides, for whom the agent acts', async ({ page, newContext }) => {
  await nadiaAsks(page, newContext);
  const marc = people.marc!;
  await marc.goto('/approvals');
  await expect(marc.getByText('Pour Nadia Benali')).toBeVisible();
  await expect(marc.getByRole('link', { name: 'voir l’exécution' })).toHaveCount(0);
  await marc.getByRole('button', { name: 'Refuser' }).click();

  // Nadia's run goes on without the action.
  await expect(page.getByText(REFUSED)).toBeVisible({ timeout: 30_000 });
  // Recorded as decided by Marc for Nadia.
  await marc.reload();
  await expect(marc.getByText('Pour Nadia Benali, décidée par Marc Lefèvre')).toBeVisible();
  await expect(marc.getByRole('button', { name: 'Refuser' })).toHaveCount(0);
  await noError(marc);
});

test('whom the agent acts for withdraws the request', async ({ page, newContext }) => {
  await nadiaAsks(page, newContext);
  await page.getByRole('button', { name: 'Retirer la demande' }).click();
  await expect(page.getByText(REFUSED)).toBeVisible({ timeout: 30_000 });

  // Recorded as withdrawn by Nadia: Marc has nothing left to decide.
  await page.goto('/approvals');
  await expect(page.getByText('Retirée par Nadia Benali')).toBeVisible();
  const marc = people.marc!;
  await marc.goto('/approvals');
  await expect(marc.getByText('Retirée par Nadia Benali')).toBeVisible();
  await expect(marc.getByRole('button', { name: 'Refuser' })).toHaveCount(0);
  await noError(page);
});

const MARC = { id: '00000000-0000-4000-8000-000000000201', displayName: 'Marc Lefèvre' };
const NADIA = { id: '00000000-0000-4000-8000-000000000205', displayName: 'Nadia Benali' };
const CLAIRE = { id: '00000000-0000-4000-8000-000000000200', displayName: 'Claire Martin' };

function request(id: string, summary: string, fields: Partial<Approval>): Approval {
  return {
    id,
    runId: `00000000-0000-4000-8000-0000000009${id.slice(-2)}`,
    toolName: 'planning_apply',
    actionSummary: summary,
    payload: { proposalId: 'p' },
    risk: 'write',
    reason: 'Action modifiant des données',
    status: 'pending',
    decisionComment: null,
    createdAt: '2026-10-21T08:00:00.000Z',
    decidedAt: null,
    expiresAt: '2026-10-28T08:00:00.000Z',
    requestedFor: MARC,
    approver: MARC,
    decidedBy: null,
    canDecide: true,
    ...fields,
  };
}

// States the demonstration does not reach (its director may not validate):
// the API's answer stands in for them.
test('requests awaiting someone else, or nobody, may be withdrawn', async ({ page }) => {
  const answer: ApprovalList = {
    approvals: [
      request('00000000-0000-4000-8000-000000000a01', 'Appliquer le planning de Marc', {
        approver: CLAIRE,
        canDecide: false,
      }),
      // Its manager's account is gone: nobody decides it any more.
      request('00000000-0000-4000-8000-000000000a02', 'Appliquer le planning de mardi', {
        approver: null,
        canDecide: false,
      }),
      request('00000000-0000-4000-8000-000000000a03', 'Appliquer le planning de Nadia', {
        requestedFor: NADIA,
      }),
    ],
  };
  await page.route('**/api/approvals?**', (route) => route.fulfill({ json: answer }));
  await page.route('**/api/approvals', (route) => route.fulfill({ json: answer }));
  await signIn(page, demo.marc);
  await page.goto('/approvals');

  await expect(page.getByText('En attente de la validation de Claire Martin.')).toBeVisible();
  await expect(
    page.getByText('Plus personne ne peut valider cette demande : retirez-la'),
  ).toBeVisible();
  await expect(page.getByRole('button', { name: 'Retirer la demande' })).toHaveCount(2);
  // One request to decide: one pair of buttons; links to Marc's own runs.
  await expect(page.getByRole('button', { name: 'Valider' })).toHaveCount(1);
  await expect(page.getByRole('button', { name: 'Refuser' })).toHaveCount(1);
  await expect(page.getByRole('link', { name: 'voir l’exécution' })).toHaveCount(2);
  // The sidebar counts what Marc may decide.
  await expect(
    page.getByRole('navigation', { name: 'Navigation principale' }).getByText('1', { exact: true }),
  ).toBeVisible();
  await noError(page);
});
