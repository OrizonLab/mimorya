import type { Page } from '@playwright/test';
import type { ApprovalList, ApprovalVerdict } from '../../lib/api/contract';
import { expect } from './fixtures';

/**
 * Refuses the validations still waiting for the page's user: a run a test
 * left waiting (it failed before deciding) ends refused. Nothing to do when
 * the page signed nobody in.
 */
export async function refuseWaiting(page: Page) {
  const approvals = await page.request.get('/api/approvals');
  if (approvals.status() !== 200) return;
  const { approvals: waiting } = (await approvals.json()) as ApprovalList;
  for (const approval of waiting.filter((a) => a.status === 'pending')) {
    const refused = await page.request.post(`/api/approvals/${approval.id}/decision`, {
      headers: { 'X-Mimorya-Request': '1' },
      data: { approved: false } satisfies ApprovalVerdict,
    });
    expect(refused.status()).toBe(200);
  }
}
