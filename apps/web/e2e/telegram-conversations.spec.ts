// A conversation with the agent held in Telegram, as the application shows
// it (ADR-135): Léa, her Telegram account linked to hers with the bot's
// consent, asks the bot a question in her private chat with it; the answer
// is written there, in the HTML Telegram reads, with the address of its
// conversation. Opened there, the conversation is listed as held in
// Telegram, her message marked as written there, and a note says what is
// answered where. She follows up: answered there again, and in the
// conversation. `/nouvelle` then leaves it: her next question opens
// another conversation. Each step is read back from the API. Whatever
// happens, the bot is deleted with her link, and her conversations of the
// questions archived. The rules themselves are tested on the API
// (`crates/api/tests/telegram_gateway_*.rs`).
import type { Page } from '@playwright/test';
import type {
  ConversationChanges,
  ConversationDetail,
  ConversationList,
} from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { deleteGateways } from './support/slack';
import { LEA_TELEGRAM, botAnswer, declareBot, linkThroughApi, storedBot } from './support/telegram';

const WRITE = { 'X-Mimorya-Request': '1' };
/** What the end-to-end model answers (`e2e_server/telegram.rs`). */
const ASKED = 'Quelles visites sont prévues vendredi à Villeurbanne ?';
const FOLLOW_UP = 'À quelle heure ?';
const FOLLOW_UP_ANSWER = 'La visite commence à 10 h.';
const NEXT = 'Et la semaine prochaine à Villeurbanne ?';
const NEXT_ANSWER = 'Rien n’est encore prévu la semaine prochaine à Villeurbanne.';
/** How long an answer of the agent's takes to be written in Telegram. */
const ANSWERED_WITHIN = 30_000;

/** Léa, once signed in: her conversations of the questions are archived after the test. */
let signedIn: Page | null = null;

test.afterEach(async ({ page }) => {
  const lea = signedIn;
  signedIn = null;
  try {
    if (lea) await archiveQuestions(lea);
  } finally {
    // Deleted even if archiving failed: the next test declares its own.
    await deleteGateways(page);
  }
});

/** Archives `lea`'s conversations of the questions. */
async function archiveQuestions(lea: Page) {
  const listed = await lea.request.get('/api/conversations');
  expect(listed.status()).toBe(200);
  for (const { id, title } of ((await listed.json()) as ConversationList).conversations) {
    if (title !== ASKED && title !== NEXT) continue;
    const archived = await lea.request.patch(`/api/conversations/${id}`, {
      headers: WRITE,
      data: { archived: true } satisfies ConversationChanges,
    });
    expect(archived.status()).toBe(200);
  }
}

/** The address of the conversation an answer in Telegram links to. */
function addressIn(answer: string): string {
  const address = /<a href="(http[^"]+)">ouvrir la conversation<\/a>/.exec(answer)?.[1] ?? '';
  expect(address, answer).toMatch(/^http:\/\/localhost:3000\/chat\/[0-9a-f-]{36}$/);
  return address;
}

test('a question asked in Telegram is answered in her chat, its conversation shown as held there', async ({
  page,
  request,
  newContext,
}) => {
  // Paul declares the company's bot; Léa links her Telegram account.
  await signIn(page, demo.paul);
  const webhook = await declareBot(page, 'Telegram e2e');
  const id = (await storedBot(page)).gateway?.id ?? '';
  const lea = await (await newContext()).newPage();
  await signIn(lea, demo.lea);
  signedIn = lea;
  await linkThroughApi(lea, webhook, id, LEA_TELEGRAM);

  // She asks the bot: answered in her chat, in HTML, with the address of
  // its conversation.
  const answer = await botAnswer(request, webhook, LEA_TELEGRAM, ASKED, ANSWERED_WITHIN);
  expect(answer).toContain('<b>Une visite</b> est prévue vendredi à Villeurbanne');
  const address = addressIn(answer);
  const conversation = address.slice(address.lastIndexOf('/') + 1);

  // Opened there: listed as held in Telegram, her message marked as
  // written there, and a note says what is answered where.
  await lea.goto(address);
  await expect(lea.getByRole('link', { name: ASKED })).toContainText('Telegram');
  await expect(lea.getByText('Écrit dans Telegram', { exact: true })).toHaveCount(1);
  await expect(lea.getByText('est prévue vendredi à Villeurbanne')).toBeVisible();
  await expect(lea.getByRole('note')).toContainText(
    'Cette conversation se tient aussi dans votre conversation privée avec le bot Telegram',
  );
  await noError(lea);
  const stored = async (held: string) => {
    const read = await lea.request.get(`/api/conversations/${held}`);
    expect(read.status()).toBe(200);
    return (await read.json()) as ConversationDetail;
  };
  const detail = await stored(conversation);
  expect(detail.conversation).toMatchObject({ title: ASKED, gateway: 'telegram' });
  expect(detail.messages[0]).toMatchObject({ content: ASKED, metadata: { gateway: 'telegram' } });

  // She follows up: answered in her chat, and in the conversation.
  const followed = await botAnswer(request, webhook, LEA_TELEGRAM, FOLLOW_UP, ANSWERED_WITHIN);
  expect(followed).toContain(FOLLOW_UP_ANSWER);
  expect(addressIn(followed)).toBe(address);
  await lea.reload();
  await expect(lea.getByText(FOLLOW_UP_ANSWER)).toBeVisible();
  await expect(lea.getByText('Écrit dans Telegram', { exact: true })).toHaveCount(2);
  await noError(lea);
  expect((await stored(conversation)).messages.map((m) => m.content)).toEqual([
    ASKED,
    expect.stringContaining('Une visite'),
    FOLLOW_UP,
    FOLLOW_UP_ANSWER,
  ]);

  // `/nouvelle` leaves it: her next question opens another conversation.
  const left = await botAnswer(request, webhook, LEA_TELEGRAM, '/nouvelle');
  expect(left).toContain('votre prochain message ouvrira une nouvelle conversation');
  const next = await botAnswer(request, webhook, LEA_TELEGRAM, NEXT, ANSWERED_WITHIN);
  expect(next).toContain(NEXT_ANSWER);
  const other = addressIn(next);
  expect(other).not.toBe(address);
  const opened = await stored(other.slice(other.lastIndexOf('/') + 1));
  expect(opened.conversation).toMatchObject({ title: NEXT, gateway: 'telegram' });
  expect((await stored(conversation)).messages).toHaveLength(4);
});
