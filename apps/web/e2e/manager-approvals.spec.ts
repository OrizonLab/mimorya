// Validation by a manager, as the web shows it (ADR-059): whom the agent
// acts for sees who will decide, with no buttons; the manager sees for whom
// the action is, decides it, and is not led to that person's run. The
// sidebar counts only what the reader may decide. The API's answer stands in
// for the requests here: the routing itself is tested on the API
// (`crates/api/tests/manager_approvals.rs`).
import type { Approval, ApprovalList } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';

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

test('the requester sees who decides; the manager decides for whom it is', async ({ page }) => {
  const answer: ApprovalList = {
    approvals: [
      // Marc's own action, sent to the director.
      request('00000000-0000-4000-8000-000000000a01', 'Appliquer le planning de Marc', {
        approver: CLAIRE,
        canDecide: false,
      }),
      // Nadia's action, which Marc decides as her manager.
      request('00000000-0000-4000-8000-000000000a02', 'Appliquer le planning de Nadia', {
        requestedFor: NADIA,
      }),
    ],
  };
  await page.route('**/api/approvals?**', (route) => route.fulfill({ json: answer }));
  await page.route('**/api/approvals', (route) => route.fulfill({ json: answer }));
  await signIn(page, demo.marc);
  await page.goto('/approvals');

  const own = page.locator('div', { hasText: 'Appliquer le planning de Marc' }).last();
  const nadias = page.locator('div', { hasText: 'Appliquer le planning de Nadia' }).last();
  await expect(page.getByText('En attente de la validation de Claire Martin.')).toBeVisible();
  await expect(page.getByText('Pour Nadia Benali')).toBeVisible();
  // Two requests, one to decide: one pair of buttons, one link to a run.
  await expect(page.getByRole('button', { name: 'Valider' })).toHaveCount(1);
  await expect(page.getByRole('button', { name: 'Refuser' })).toHaveCount(1);
  await expect(page.getByRole('link', { name: 'voir l’exécution' })).toHaveCount(1);
  await expect(own).toBeVisible();
  await expect(nadias).toBeVisible();
  // The sidebar counts what Marc may decide.
  await expect(
    page.getByRole('navigation', { name: 'Navigation principale' }).getByText('1', { exact: true }),
  ).toBeVisible();
  await noError(page);
});
