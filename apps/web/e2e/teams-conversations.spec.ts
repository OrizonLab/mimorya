// A conversation with the agent held in Teams, as the application shows it
// (ADR-140): Léa, her Teams account linked to hers from the bot's link,
// asks the bot a question in her personal conversation with it, as the
// test Bot Framework sends it; the answer is written there, in Teams'
// Markdown, with the link to its conversation. Opened there, the
// conversation is listed as held in Teams, her message marked as written
// there, and a note says what is answered where. She follows up: answered
// there again, and in the conversation. `/nouvelle` then leaves it, held
// there no more: her next question opens another conversation. Each step is
// read back from the API. Whatever happens, the bot is deleted with her
// link, and her conversations of the questions archived. The rules
// themselves are tested on the API (`crates/api/tests/teams_gateway_*.rs`).
import type { Page } from '@playwright/test';
import type {
  ConversationChanges,
  ConversationDetail,
  ConversationList,
} from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import { deleteGateways } from './support/slack';
import { LEA_TEAMS, botAnswer, declareTeamsBot, linkThroughBot } from './support/teams';

const WRITE = { 'X-Mimorya-Request': '1' };
/** What the end-to-end model answers (`e2e_server/teams.rs`). */
const ASKED = 'Quelles visites sont prévues jeudi à Vénissieux ?';
const FOLLOW_UP = 'Laquelle commence en premier ?';
const FOLLOW_UP_ANSWER = 'Le contrôle de l’ascenseur, à 8 h 30.';
const NEXT = 'Et vendredi à Vénissieux ?';
const NEXT_ANSWER = 'Rien n’est encore prévu vendredi à Vénissieux.';
/** How long an answer of the agent's takes to be written in Teams. */
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

/** The address of the conversation an answer in Teams links to. */
function addressIn(answer: string): string {
  const address = /\[ouvrir la conversation\]\((http[^)]+)\)/.exec(answer)?.[1] ?? '';
  expect(address, answer).toMatch(/^http:\/\/localhost:3000\/chat\/[0-9a-f-]{36}$/);
  return address;
}

/** `text` as Teams' Markdown writes it: its full stops escaped. */
function written(text: string): string {
  return text.replaceAll('.', '\\.');
}

test('a question asked in Teams is answered in her conversation with the bot, its conversation shown as held there', async ({
  page,
  request,
  newContext,
}) => {
  // Paul declares the company's bot; Léa links her Teams account.
  await signIn(page, demo.paul);
  const endpoint = await declareTeamsBot(page, 'Teams e2e');
  const lea = await (await newContext()).newPage();
  await signIn(lea, demo.lea);
  signedIn = lea;
  await linkThroughBot(lea, endpoint, LEA_TEAMS);

  // She asks the bot: answered in her conversation with it, in Markdown,
  // with the link to its conversation.
  const answer = await botAnswer(request, endpoint, LEA_TEAMS, ASKED, ANSWERED_WITHIN);
  expect(answer).toContain('**Deux visites** sont prévues jeudi à Vénissieux');
  expect(answer).toContain('*Dans l’application : [ouvrir la conversation](');
  const address = addressIn(answer);
  const conversation = address.slice(address.lastIndexOf('/') + 1);

  // Opened there: listed as held in Teams, her message marked as written
  // there, and a note says what is answered where.
  await lea.goto(address);
  await expect(lea.getByRole('link', { name: ASKED })).toContainText('Teams');
  await expect(lea.getByText('Écrit dans Teams', { exact: true })).toHaveCount(1);
  await expect(lea.getByText('l’entretien d’une chaudière', { exact: false })).toBeVisible();
  await expect(lea.getByRole('note')).toContainText(
    'Cette conversation se tient aussi dans votre conversation avec le bot Teams',
  );
  await noError(lea);
  const stored = async (held: string) => {
    const read = await lea.request.get(`/api/conversations/${held}`);
    expect(read.status()).toBe(200);
    return (await read.json()) as ConversationDetail;
  };
  const detail = await stored(conversation);
  expect(detail.conversation).toMatchObject({ title: ASKED, gateway: 'teams' });
  expect(detail.messages[0]).toMatchObject({ content: ASKED, metadata: { gateway: 'teams' } });

  // She follows up: answered in her conversation with the bot, and in the
  // conversation.
  const followed = await botAnswer(request, endpoint, LEA_TEAMS, FOLLOW_UP, ANSWERED_WITHIN);
  expect(followed).toContain(written(FOLLOW_UP_ANSWER));
  expect(addressIn(followed)).toBe(address);
  await lea.reload();
  await expect(lea.getByText(FOLLOW_UP_ANSWER)).toBeVisible();
  await expect(lea.getByText('Écrit dans Teams', { exact: true })).toHaveCount(2);
  await noError(lea);
  expect((await stored(conversation)).messages.map((m) => m.content)).toEqual([
    ASKED,
    expect.stringContaining('Deux visites'),
    FOLLOW_UP,
    FOLLOW_UP_ANSWER,
  ]);

  // `/nouvelle` leaves it: her next question opens another conversation.
  const left = await botAnswer(request, endpoint, LEA_TEAMS, '/nouvelle');
  expect(left).toContain('votre prochain message ouvrira une nouvelle conversation');
  const next = await botAnswer(request, endpoint, LEA_TEAMS, NEXT, ANSWERED_WITHIN);
  expect(next).toContain(written(NEXT_ANSWER));
  const other = addressIn(next);
  expect(other).not.toBe(address);
  const opened = await stored(other.slice(other.lastIndexOf('/') + 1));
  expect(opened.conversation).toMatchObject({ title: NEXT, gateway: 'teams' });
  // The first is held there no more, her messages still marked.
  const former = await stored(conversation);
  expect(former.conversation.gateway).toBeNull();
  expect(former.messages).toHaveLength(4);
  expect(former.messages[0]).toMatchObject({ metadata: { gateway: 'teams' } });
});
