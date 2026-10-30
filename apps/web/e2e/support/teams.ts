// The company's Teams bot in the end-to-end tests (ADR-138, ADR-139): the
// test Microsoft the server reaches (`e2e_server`), whose Entra knows the
// demo bot's application and its secret, whose Teams service knows Léa
// and lists what the bots wrote, and which signs the activity a person's
// message would be; that activity sent to the bot's messaging endpoint, as
// the Bot Framework sends it, what the bot answers and the link it offers;
// the bot read back from the API.
import type { APIRequestContext, APIResponse, Page } from '@playwright/test';
import type { GatewayView, Gateways } from '../../lib/api/contract';
import { expect } from './fixtures';

/** The demo bot's application, its directory and its client secret. */
export const TEAMS_APP = '6f1c2b7a-0d3e-4c5b-9a8f-7e6d5c4b3a21';
export const TEAMS_DIRECTORY = '0b2c3d4e-5f60-4718-8293-a4b5c6d7e8f9';
export const TEAMS_SECRET = 'Abc8Q~fictif.secret_client-0123456789';
/** Léa's Microsoft Entra object, and her name, as Teams knows them. */
export const LEA_TEAMS = { aad: '3c4d5e6f-7a8b-4c9d-8e0f-1a2b3c4d5e6f', name: 'Léa Fontaine' };

const MICROSOFT = 'http://127.0.0.1:4105';

interface Written {
  app: string;
  conversation: string;
  text: string;
}

interface Signed {
  authorization: string;
  activity: unknown;
}

/** The messages the bots wrote so far, oldest first. */
async function written(outside: APIRequestContext): Promise<Written[]> {
  const listed = await outside.get(`${MICROSOFT}/test/messages`);
  expect(listed.status()).toBe(200);
  return (await listed.json()) as Written[];
}

/** The activities sent so far, numbered from now: never one taken before. */
let sent = 0;

/**
 * What the Bot Framework sends the messaging endpoint `url` when `person`
 * writes `text` to the bot in their personal conversation with it: the
 * activity, with its token. No session.
 */
export async function sendActivity(
  outside: APIRequestContext,
  url: string,
  person: { aad: string; name: string },
  text: string,
): Promise<APIResponse> {
  sent += 1;
  const signed = await outside.post(`${MICROSOFT}/test/activity`, {
    data: { app: TEAMS_APP, id: `e2e-${Date.now()}-${sent}`, ...person, text },
  });
  expect(signed.status()).toBe(200);
  const { authorization, activity } = (await signed.json()) as Signed;
  return outside.post(url, { headers: { Authorization: authorization }, data: activity });
}

/** What the bot answers `person` writing `text`, once it took the activity. */
export async function botAnswer(
  outside: APIRequestContext,
  url: string,
  person: { aad: string; name: string },
  text: string,
): Promise<string> {
  const before = (await written(outside)).length;
  const taken = await sendActivity(outside, url, person, text);
  expect(taken.status()).toBe(200);
  let messages: Written[] = [];
  await expect
    .poll(async () => {
      messages = await written(outside);
      return messages.length;
    })
    .toBeGreaterThan(before);
  const answer = messages[messages.length - 1];
  expect(answer).toMatchObject({ app: TEAMS_APP, conversation: `a:conversation-${person.aad}` });
  return answer?.text ?? '';
}

/** The link to the person's page an answer of the bot offers, in Markdown. */
export function offeredLink(answer: string): string {
  const link = /\]\((http[^)\s]+)\)/.exec(answer)?.[1];
  expect(link, answer).toBeTruthy();
  return link ?? '';
}

/** The company's Teams bot as the API records it, and the whole listing. */
export async function storedTeamsBot(
  page: Page,
): Promise<{ gateway?: GatewayView; listing: string }> {
  const listed = await page.request.get('/api/admin/gateways');
  expect(listed.status()).toBe(200);
  const listing = await listed.text();
  const { gateways } = JSON.parse(listing) as Gateways;
  return { gateway: gateways.find((g) => g.type === 'teams'), listing };
}
