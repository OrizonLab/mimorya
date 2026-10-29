// The company's Telegram bot in the end-to-end tests (ADR-133, ADR-134):
// the test Telegram the server reaches (`e2e_server`), which knows the
// demo bot, keeps the webhook each bot was given with its secret and lists
// what the bots wrote; an update sent as Telegram sends it, with that
// secret, and what the bot answers it; the bot declared, its link read in
// « Mon compte », the gateway read back from the API, a person's Telegram
// account linked through the API (ADR-135).
import type { APIRequestContext, APIResponse, Locator, Page } from '@playwright/test';
import type { GatewayView, Gateways, TelegramLink } from '../../lib/api/contract';
import { expect } from './fixtures';

/** The demo bot's token, as the test Telegram knows it, and the bot it is. */
export const TELEGRAM_TOKEN = '123456789:AAH-fictif_jeton_du_bot_neo_maint_0001';
export const BOT_ID = 123456789;
export const BOT_USERNAME = 'NeoMaintenanceBot';
/** Léa's Telegram account, and Marc's. */
export const LEA_TELEGRAM = { id: 4242, username: 'lea_e2e' };
export const MARC_TELEGRAM = { id: 5151, username: 'marc_e2e' };

const TELEGRAM = 'http://127.0.0.1:4104';

interface Webhook {
  bot: number;
  url: string;
  secret: string;
}

interface Message {
  bot: number;
  chat: number;
  text: string;
}

/** The webhook the bot was given, with its secret, if any. */
export async function webhookOf(outside: APIRequestContext): Promise<Webhook | undefined> {
  const listed = await outside.get(`${TELEGRAM}/test/webhooks`);
  expect(listed.status()).toBe(200);
  return ((await listed.json()) as Webhook[]).find((w) => w.bot === BOT_ID);
}

/** The messages the bots wrote so far, oldest first. */
async function messages(outside: APIRequestContext): Promise<Message[]> {
  const listed = await outside.get(`${TELEGRAM}/test/messages`);
  expect(listed.status()).toBe(200);
  return (await listed.json()) as Message[];
}

/** The updates sent so far, numbered from now: never one taken before. */
let sent = 0;

/**
 * What Telegram sends `url` when `person` writes `text` in their private
 * chat with the bot: the update, with the webhook's `secret` in its
 * header. No session.
 */
export function sendUpdate(
  outside: APIRequestContext,
  { url, secret }: Webhook,
  person: { id: number; username: string },
  text: string,
): Promise<APIResponse> {
  sent += 1;
  const updateId = Math.floor(Date.now() / 1000) * 100 + sent;
  return outside.post(url, {
    headers: { 'X-Telegram-Bot-Api-Secret-Token': secret },
    data: {
      update_id: updateId,
      message: {
        message_id: updateId,
        from: { id: person.id, is_bot: false, first_name: 'Léa', username: person.username },
        chat: { id: person.id, type: 'private', first_name: 'Léa' },
        date: Math.floor(Date.now() / 1000),
        text,
      },
    },
  });
}

/**
 * What the bot answers `person` writing `text`, once Telegram took the
 * update: within `timeout`, which an answer of the agent's, written by
 * the pass of every few seconds (ADR-135), needs longer.
 */
export async function botAnswer(
  outside: APIRequestContext,
  webhook: Webhook,
  person: { id: number; username: string },
  text: string,
  timeout = 5_000,
): Promise<string> {
  const before = (await messages(outside)).length;
  const taken = await sendUpdate(outside, webhook, person, text);
  expect(taken.status()).toBe(200);
  let written: Message[] = [];
  await expect
    .poll(
      async () => {
        written = await messages(outside);
        return written.length;
      },
      { timeout },
    )
    .toBeGreaterThan(before);
  const answer = written[written.length - 1];
  expect(answer?.chat).toBe(person.id);
  return answer?.text ?? '';
}

/** The company's Telegram bot as the API records it, and the whole listing. */
export async function storedBot(page: Page): Promise<{ gateway?: GatewayView; listing: string }> {
  const listed = await page.request.get('/api/admin/gateways');
  expect(listed.status()).toBe(200);
  const listing = await listed.text();
  const { gateways } = JSON.parse(listing) as Gateways;
  return { gateway: gateways.find((g) => g.type === 'telegram'), listing };
}

/** The bot `name`, declared by `admin`, signed in, through the API. */
export async function declareBot(admin: Page, name: string): Promise<Webhook> {
  const declared = await admin.request.post('/api/admin/gateways/telegram', {
    headers: { 'X-Mimorya-Request': '1' },
    data: { name, botToken: TELEGRAM_TOKEN },
  });
  expect(declared.status()).toBe(201);
  const webhook = await webhookOf(admin.request);
  expect(webhook).toBeTruthy();
  return webhook as Webhook;
}

/** The bot's link in `row`, shown or not. */
export function botLink(row: Locator): Locator {
  return row.getByRole('link', { name: `Ouvrir le bot @${BOT_USERNAME} dans Telegram` });
}

/** The bot's link `row` shows, once shown. */
export async function shownLink(row: Locator): Promise<string> {
  const opened = botLink(row);
  await expect(opened).toBeVisible();
  return (await opened.getAttribute('href')) ?? '';
}

/** The code the bot's `link` carries. */
export function codeOf(link: string): string {
  const code = new RegExp(`^https://t\\.me/${BOT_USERNAME}\\?start=([A-Za-z0-9_-]{43})$`).exec(
    link,
  )?.[1];
  expect(code, link).toBeTruthy();
  return code ?? '';
}

/** What `person` has under way on the bot, as the API lists it, and the listing. */
export async function pendingOf(person: Page): Promise<{ pending: unknown; listing: string }> {
  const listed = await person.request.get('/api/account/gateways');
  expect(listed.status()).toBe(200);
  const listing = await listed.text();
  const { gateways } = JSON.parse(listing) as { gateways: { type: string; pending: unknown }[] };
  return { pending: gateways.find((g) => g.type === 'telegram')?.pending, listing };
}

/**
 * Links `person`'s Telegram account `telegram` to theirs on the bot `id`,
 * through the API, as « Mon compte » does: the code asked for, opened and
 * accepted in Telegram, confirmed.
 */
export async function linkThroughApi(
  person: Page,
  webhook: Webhook,
  id: string,
  telegram: { id: number; username: string },
) {
  const write = { 'X-Mimorya-Request': '1' };
  const asked = await person.request.post(`/api/account/gateways/${id}/telegram-code`, {
    headers: write,
  });
  expect(asked.status()).toBe(200);
  const code = codeOf(((await asked.json()) as TelegramLink).link);
  const offered = await botAnswer(person.request, webhook, telegram, `/start ${code}`);
  expect(offered).toContain('envoyez /oui');
  await botAnswer(person.request, webhook, telegram, '/oui');
  const confirmed = await person.request.post(`/api/account/gateways/${id}/telegram-link`, {
    headers: write,
  });
  expect(confirmed.status()).toBe(200);
}
