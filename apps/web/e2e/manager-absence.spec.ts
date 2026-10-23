// An absent manager names who validates in their stead (ADR-071 to ADR-073):
// Marc declares his absence in « Validations », naming Julien, whom Paul
// gave the right over Nadia's team (naming him grants none). Nadia's request
// goes to Marc: Julien is told by e-mail, in Marc's absence, finds it among
// his own and refuses it; Marc then ends his absence. Whatever happens, the
// absence, the role and the routing are undone. The rules themselves are
// tested on the API (`crates/api/tests/approval_delegation*.rs`).
import type { Page } from '@playwright/test';
import type {
  ApprovalDelegations,
  ApprovalList,
  ApprovalRoutingUpdate,
  ApprovalVerdict,
  RoleGrant,
  ToolRisk,
} from '../lib/api/contract';
import { inDays } from './support/dates';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { lastLink, mailCount } from './support/mail';
import { REFERENCE_REQUEST } from './support/scenario';

const WRITE = { 'X-Mimorya-Request': '1' };
const JULIEN = '00000000-0000-4000-8000-000000000202';
const NADIA = '00000000-0000-4000-8000-000000000205';
const TEAM_LEAD = '00000000-0000-4000-8000-000000000311';
const TEAM_SOUTH = '00000000-0000-4000-8000-000000000104';
const REFUSED = 'Le planning est inchangé : la validation a été refusée.';

/** Who the test signed in, and the role it gave, for its cleanup. */
const left: { paul?: Page; marc?: Page; julien?: Page; assignment?: string } = {};

async function routeToManagers(paul: Page, risks: ToolRisk[]) {
  const set = await paul.request.put('/api/admin/approval-routing', {
    headers: WRITE,
    data: { managerRisks: risks } satisfies ApprovalRoutingUpdate,
  });
  expect(set.status()).toBe(200);
}

/**
 * Refuses Nadia's requests `page`'s user may still decide: a failed test
 * leaves one waiting. Marc's own, which the server leaves for
 * `interrupted-run.spec.ts`, stays as it is.
 */
async function refuseNadias(page: Page) {
  const listed = await page.request.get('/api/approvals');
  if (listed.status() !== 200) return;
  const { approvals } = (await listed.json()) as ApprovalList;
  for (const a of approvals.filter((a) => a.canDecide && a.requestedFor.id === NADIA)) {
    const refused = await page.request.post(`/api/approvals/${a.id}/decision`, {
      headers: WRITE,
      data: { approved: false } satisfies ApprovalVerdict,
    });
    expect(refused.status()).toBe(200);
  }
}

async function own(marc: Page) {
  const read = await marc.request.get('/api/approval-delegation');
  expect(read.status()).toBe(200);
  return ((await read.json()) as ApprovalDelegations).own;
}

test.afterEach(async () => {
  const { paul, marc, julien, assignment } = left;
  // What a failed test left waiting ends refused.
  if (julien) await refuseNadias(julien);
  if (marc) {
    await refuseNadias(marc);
    const ended = await marc.request.delete('/api/approval-delegation', { headers: WRITE });
    expect([200, 404]).toContain(ended.status());
  }
  if (paul) {
    await routeToManagers(paul, []);
    if (assignment) {
      const revoked = await paul.request.delete(`/api/admin/assignments/${assignment}`, {
        headers: WRITE,
      });
      expect(revoked.status()).toBe(200);
    }
  }
  left.paul = left.marc = left.julien = left.assignment = undefined;
});

test('an absent manager names who validates in their stead', async ({ page, newContext }) => {
  const paul = (left.paul = await (await newContext()).newPage());
  const marc = (left.marc = await (await newContext()).newPage());
  const julien = (left.julien = await (await newContext()).newPage());
  await signIn(paul, demo.paul);
  await signIn(marc, demo.marc);
  await signIn(julien, demo.julien);

  // Paul gives Julien the right over Nadia's team.
  const granted = await paul.request.post(`/api/admin/users/${JULIEN}/assignments`, {
    headers: WRITE,
    data: {
      roleId: TEAM_LEAD,
      scope: { type: 'org_unit', orgUnitId: TEAM_SOUTH, includeDescendants: true },
    } satisfies RoleGrant,
  });
  expect(granted.status()).toBe(201);
  left.assignment = ((await granted.json()) as { id: string }).id;

  // Marc declares his absence, Julien standing in, as the API records it.
  await marc.goto('/approvals');
  await marc.getByLabel('Remplaçant').selectOption({ label: 'Julien Moreau' });
  await marc.getByLabel('Au (inclus)').fill(inDays(3));
  await marc.getByRole('button', { name: 'Déclarer mon absence' }).click();
  await expect(marc.getByRole('button', { name: 'Mettre fin à l’absence' })).toBeVisible();
  const declared = await own(marc);
  expect(declared?.delegate.id).toBe(JULIEN);
  // Until the end of the last day, in the browser's time zone (Node's too).
  expect(declared?.endsAt).toBe(new Date(`${inDays(3)}T23:59:59`).toISOString());
  await marc.reload();
  await expect(marc.getByText(/^Julien Moreau vous remplace du /)).toBeVisible();

  // Nadia's request awaits Marc: Julien is told, in Marc's absence.
  const before = await mailCount(demo.julien);
  await routeToManagers(paul, ['write']);
  await signIn(page, demo.nadia);
  await page.getByLabel('Votre message').fill(REFERENCE_REQUEST);
  await page.getByRole('button', { name: 'Envoyer' }).click();
  await expect(page.getByText('En attente de la validation de Marc Lefèvre.')).toBeVisible({
    timeout: 30_000,
  });
  const { link, mail } = await lastLink(demo.julien, 'approvals', before);
  expect(mail.subject).toBe(
    'Une action de Nadia Benali attend votre validation (en l’absence de Marc Lefèvre)',
  );
  await julien.goto(link);
  await expect(julien.getByText(/^Vous remplacez Marc Lefèvre du /)).toBeVisible();
  await expect(julien.getByText('En l’absence de Marc Lefèvre')).toBeVisible();
  await julien.getByRole('button', { name: 'Refuser' }).click();

  // Nadia's run goes on without the action; Julien's decision is recorded.
  await expect(page.getByText(REFUSED)).toBeVisible({ timeout: 30_000 });
  await julien.reload();
  await expect(julien.getByText('Pour Nadia Benali, décidée par Julien Moreau')).toBeVisible();

  // Back, Marc ends his absence.
  await marc.getByRole('button', { name: 'Mettre fin à l’absence' }).click();
  await expect(marc.getByRole('button', { name: 'Déclarer mon absence' })).toBeVisible();
  expect(await own(marc)).toBeNull();
  await noError(marc);
  await noError(julien);
});
