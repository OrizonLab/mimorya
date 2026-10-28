// The company's Slack gateway in the end-to-end tests (ADR-125, ADR-126):
// the test Slack the server reaches (`e2e_server`), which knows Léa's Slack
// user and lists what the application posted; an event signed as Slack
// signs them, with the application's secret; the gateway read back from the
// API, and deleted after each test with the identities linked there.
import { createHmac } from 'node:crypto';
import type { APIRequestContext, APIResponse, Page } from '@playwright/test';
import type { GatewayView, Gateways } from '../../lib/api/contract';
import { expect } from './fixtures';

/** The application's bot token and signing secret, as the test Slack knows them. */
export const BOT_TOKEN = 'xoxb-fictif-bot-neo-maintenance-0001';
export const SIGNING_SECRET = '5ec2e75ec2e75ec2e75ec2e75ec2e75e';
/** The workspace the token belongs to. */
export const WORKSPACE = 'T0NEOMAINT';
export const WORKSPACE_NAME = 'Néo Maintenance';
/** Léa's Slack user, whose address is her account's. */
export const LEA_SLACK = 'U0LEAE2E';
/** What the test Slack lists of the messages the application posted. */
const POSTS = 'http://127.0.0.1:4103/test/posts';

const WRITE = { 'X-Mimorya-Request': '1' };

interface Post {
  channel: string;
  threadTs: string | null;
  text: string;
}

/** Deletes the gateways a test declared, with their identities. */
export async function deleteGateways(page: Page) {
  const listed = await page.request.get('/api/admin/gateways');
  // 401: the test never signed in; 403: not as an administrator. Either
  // way, it declared nothing.
  if (listed.status() === 401 || listed.status() === 403) return;
  expect(listed.status()).toBe(200);
  for (const gateway of ((await listed.json()) as Gateways).gateways) {
    const deleted = await page.request.delete(`/api/admin/gateways/${gateway.id}`, {
      headers: WRITE,
    });
    expect(deleted.status()).toBe(200);
  }
}

/** The company's Slack gateway as the API records it, and the whole listing. */
export async function storedGateway(
  page: Page,
): Promise<{ gateway?: GatewayView; listing: string }> {
  const listed = await page.request.get('/api/admin/gateways');
  expect(listed.status()).toBe(200);
  const listing = await listed.text();
  const { gateways } = JSON.parse(listing) as Gateways;
  return { gateway: gateways.find((g) => g.type === 'slack'), listing };
}

/**
 * What Slack sends `url` when `person` writes `text` to the application:
 * the event, signed now with the application's secret, as Slack signs it
 * (`v0=` and the HMAC-SHA256 of `v0:<timestamp>:<body>`). No session.
 */
export async function sendDirectMessage(
  outside: APIRequestContext,
  url: string,
  person: string,
  text: string,
): Promise<APIResponse> {
  const now = Math.floor(Date.now() / 1000);
  const body = JSON.stringify({
    token: 'verification-token-unused',
    team_id: WORKSPACE,
    api_app_id: 'A0MIMORYA',
    event: {
      type: 'message',
      channel: 'D0LEAE2E',
      user: person,
      text,
      ts: `${now}.000100`,
      channel_type: 'im',
      event_ts: `${now}.000100`,
    },
    type: 'event_callback',
    // Never taken before: Slack's own sending again is ignored.
    event_id: `Ev${Date.now()}`,
    event_time: now,
  });
  const timestamp = String(now);
  const signature = createHmac('sha256', SIGNING_SECRET)
    .update(`v0:${timestamp}:${body}`)
    .digest('hex');
  return outside.post(url, {
    headers: {
      'Content-Type': 'application/json',
      'X-Slack-Request-Timestamp': timestamp,
      'X-Slack-Signature': `v0=${signature}`,
    },
    data: body,
  });
}

/** The messages the application posted so far, oldest first. */
export async function posts(outside: APIRequestContext): Promise<Post[]> {
  const listed = await outside.get(POSTS);
  expect(listed.status()).toBe(200);
  return (await listed.json()) as Post[];
}

/** The link the application posts once more than `before` messages were
 * posted: the address in its Slack-formatted `<url|label>`. */
export async function offeredLink(outside: APIRequestContext, before: number): Promise<string> {
  let posted: Post[] = [];
  await expect
    .poll(async () => {
      posted = await posts(outside);
      return posted.length;
    })
    .toBeGreaterThan(before);
  const text = posted[posted.length - 1]?.text ?? '';
  const link = /<(http[^|>]+)\|/.exec(text)?.[1];
  expect(link).toBeTruthy();
  return link ?? '';
}
