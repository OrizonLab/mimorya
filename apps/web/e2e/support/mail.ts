import { readdir, readFile } from 'node:fs/promises';
import path from 'node:path';
import { MAIL_DIR } from '../../playwright.config';

interface Mail {
  to: string;
  toName: string;
  subject: string;
  text: string;
}

async function mails(): Promise<Mail[]> {
  const names = (await readdir(MAIL_DIR).catch(() => [] as string[]))
    .filter((name) => name.endsWith('.json') && !name.startsWith('.'))
    .sort();
  return Promise.all(
    names.map(
      async (name) => JSON.parse(await readFile(path.join(MAIL_DIR, name), 'utf8')) as Mail,
    ),
  );
}

/**
 * The link to `page` in the last e-mail sent to `to` (the API writes them to
 * `E2E_MAIL_DIR`), skipping the first `after` ones and waiting for it: some
 * are sent in the background. Invitation and reset links carry a token after
 * `#`; the approvals link does not.
 */
export async function lastLink(
  to: string,
  page: 'invitation' | 'reset-password' | 'approvals',
  after = 0,
) {
  for (let attempt = 0; attempt < 50; attempt++) {
    const received = (await mails()).filter((mail) => mail.to === to).slice(after);
    for (const mail of received.reverse()) {
      const link = mail.text
        .split('\n')
        .find((line) => line.includes(`/${page}#`) || line.trim().endsWith(`/${page}`));
      if (link) return { link: link.trim(), mail };
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`no ${page} link sent to ${to}`);
}

/** How many e-mails `to` received so far. */
export async function mailCount(to: string): Promise<number> {
  return (await mails()).filter((mail) => mail.to === to).length;
}
