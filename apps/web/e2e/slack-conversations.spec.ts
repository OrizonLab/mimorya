// A conversation with the agent held in Slack, as the application shows it
// (ADR-127, ADR-128): Léa, her Slack account linked from the link the
// company's application sends her, asks it a question in Slack; the answer
// is written in the thread of her message, in Slack's markup, with the
// address of its conversation. Opened there, the conversation is listed as
// held in Slack, her message marked as written there, and a note says that
// what is written in the application is answered there only. She follows
// up in the thread: answered there again, and in the conversation. Each
// step is read back from the API. Whatever happens, the gateway is deleted
// with her identity, and the conversation archived. The rules themselves
// are tested on the API (`crates/api/tests/slack_gateway_*.rs`).
import type { Page } from '@playwright/test';
import type { ConversationChanges, ConversationDetail, NewSlackGateway } from '../lib/api/contract';
import { demo, expect, noError, signIn, test } from './support/fixtures';
import {
  BOT_TOKEN,
  LEA_SLACK,
  SIGNING_SECRET,
  deleteGateways,
  inThread,
  offeredLink,
  posts,
  sendDirectMessage,
  slackTs,
  storedGateway,
} from './support/slack';

const WRITE = { 'X-Mimorya-Request': '1' };
/** What the end-to-end model answers (`e2e_server/slack.rs`). */
const ASKED = 'Quelles interventions sont prévues demain à Lyon ?';
const FOLLOW_UP = 'Laquelle commence en premier ?';
const FOLLOW_UP_ANSWER = 'La mise en service commence la première, à 8 h.';

/** Léa's conversation the test opened, archived after it. */
let opened: { page: Page; id: string } | null = null;

test.afterEach(async ({ page }) => {
  if (opened) {
    const archived = await opened.page.request.patch(`/api/conversations/${opened.id}`, {
      headers: WRITE,
      data: { archived: true } satisfies ConversationChanges,
    });
    expect(archived.status()).toBe(200);
    opened = null;
  }
  await deleteGateways(page);
});

test('a question asked in Slack is answered in its thread, its conversation shown as held there', async ({
  page,
  request,
  newContext,
}) => {
  // Paul declares the company's Slack application.
  await signIn(page, demo.paul);
  const declared = await page.request.post('/api/admin/gateways/slack', {
    headers: WRITE,
    data: {
      name: 'Slack e2e',
      botToken: BOT_TOKEN,
      signingSecret: SIGNING_SECRET,
    } satisfies NewSlackGateway,
  });
  expect(declared.status()).toBe(201);
  const eventsUrl = (await storedGateway(page)).gateway?.eventsUrl ?? '';

  // Léa links her Slack account from the link the application sends her.
  const lea = await (await newContext()).newPage();
  await signIn(lea, demo.lea);
  const before = (await posts(request)).length;
  expect((await sendDirectMessage(request, eventsUrl, LEA_SLACK, 'Bonjour')).status()).toBe(200);
  await lea.goto(await offeredLink(request, before));
  await lea.getByRole('button', { name: 'Relier mon compte Slack' }).click();
  await expect(lea.getByRole('status')).toHaveText('Votre compte Slack est relié à votre compte.');

  // She asks in Slack: answered in the thread of her message, in Slack's
  // markup, with the address of its conversation.
  const asked = slackTs();
  const sent = await sendDirectMessage(request, eventsUrl, LEA_SLACK, ASKED, { ts: asked });
  expect(sent.status()).toBe(200);
  const [answer = ''] = await inThread(request, asked, 1);
  expect(answer).toContain('*Deux interventions* sont prévues demain à Lyon');
  const address = /<(http[^|>]+)\|ouvrir la conversation>/.exec(answer)?.[1] ?? '';
  expect(address).toMatch(/^http:\/\/localhost:3000\/chat\/[0-9a-f-]{36}$/);
  const id = address.slice(address.lastIndexOf('/') + 1);
  opened = { page: lea, id };

  // Opened there: listed as held in Slack, her message marked as written
  // there, and a note says what is answered where.
  await lea.goto(address);
  const listed = lea.getByRole('link', { name: ASKED });
  await expect(listed).toContainText('Slack');
  await expect(lea.getByText('Écrit dans Slack', { exact: true })).toHaveCount(1);
  await expect(
    lea.getByText('sont prévues demain à Lyon : une mise en service le matin'),
  ).toBeVisible();
  await expect(lea.getByRole('note')).toContainText(
    'Cette conversation se tient aussi dans un fil Slack',
  );
  await noError(lea);
  const stored = async () => {
    const read = await lea.request.get(`/api/conversations/${id}`);
    expect(read.status()).toBe(200);
    return (await read.json()) as ConversationDetail;
  };
  const detail = await stored();
  expect(detail.conversation).toMatchObject({ title: ASKED, gateway: 'slack' });
  expect(detail.messages[0]).toMatchObject({ content: ASKED, metadata: { gateway: 'slack' } });

  // She follows up in the thread: answered there, and in the conversation.
  const followed = await sendDirectMessage(request, eventsUrl, LEA_SLACK, FOLLOW_UP, {
    ts: slackTs(),
    threadTs: asked,
  });
  expect(followed.status()).toBe(200);
  const said = await inThread(request, asked, 2);
  expect(said[1]).toContain(FOLLOW_UP_ANSWER);
  await lea.reload();
  await expect(lea.getByText(FOLLOW_UP_ANSWER)).toBeVisible();
  await expect(lea.getByText('Écrit dans Slack', { exact: true })).toHaveCount(2);
  await noError(lea);
  const followedUp = await stored();
  expect(followedUp.messages.map((m) => m.content)).toEqual([
    ASKED,
    expect.stringContaining('Deux interventions'),
    FOLLOW_UP,
    FOLLOW_UP_ANSWER,
  ]);
});
