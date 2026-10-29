'use client';

import { ExternalLink } from 'lucide-react';
import { useState } from 'react';
import { Badge, Button, ErrorNote } from '@/components/ui';
import { api, useAction, type AccountGatewayView, type TelegramLink } from '@/lib/api';
import { formatDate, formatTime } from '@/lib/format';

/** The row's buttons, to show the wait on the one pressed. */
type Pressed = 'ask' | 'confirm' | 'refuse' | 'cancel' | 'unlink';

/**
 * The signed-in person's Telegram account on the company's bot (ADR-133,
 * ADR-134): linked from here. They ask for the bot's link, open it in
 * Telegram, where the bot tells them whose account it links and they
 * accept it (`/oui`); back here, they see which Telegram account accepted
 * it, by its name and its identifier, and confirm, or refuse it. The link
 * is shown once, as the server gives it (`link`, kept by the list), while
 * it carries the code under way: lost or replaced, a new one is asked.
 * Whatever an action answers, the row is read again.
 */
export function TelegramIdentityRow({
  gateway,
  link,
  onLink,
  onChanged,
}: {
  gateway: AccountGatewayView;
  link: TelegramLink | null;
  onLink: (link: TelegramLink | null) => void;
  onChanged: () => void;
}) {
  const action = useAction();
  const [pressed, setPressed] = useState<Pressed | null>(null);
  const loading = (button: Pressed) => action.pending && pressed === button;
  const held = (button: Pressed) => action.pending && pressed !== button;
  const path = `/account/gateways/${gateway.gatewayId}`;
  const { linked, pending } = gateway;
  const claimedBy = pending?.claimedBy ?? null;
  // The link kept is shown only while it carries the code under way: another
  // asked meanwhile (from another page) replaced it, and it is then lost.
  const shown = link !== null && pending?.expiresAt === link.expiresAt ? link : null;

  async function run(button: Pressed, call: () => Promise<unknown>) {
    setPressed(button);
    const done = await action.run(call);
    // The link only lasts until the code is taken or forgotten.
    if (done && button !== 'ask') onLink(null);
    onChanged();
  }

  async function ask() {
    setPressed('ask');
    const asked = await action.run(() => api.post<TelegramLink>(`${path}/telegram-code`));
    if (asked) onLink(asked);
    onChanged();
  }

  async function unlink() {
    const question = `Retirer la liaison de votre compte Telegram au bot @${gateway.workspaceName} ? Vous pourrez le relier de nouveau d’ici.`;
    if (!window.confirm(question)) return;
    await run('unlink', () => api.delete(path));
  }

  return (
    <section aria-label={gateway.name} className="flex flex-col gap-2 border-t border-line pt-3">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 text-sm font-medium">{gateway.name}</p>
        <Badge tone={linked ? 'ok' : pending ? 'warn' : 'neutral'}>
          {linked ? 'Relié' : pending ? 'Liaison en cours' : 'Non relié'}
        </Badge>
      </div>
      {linked ? (
        <p className="text-sm text-muted">
          Votre compte Telegram {linked.name} (identifiant <code>{linked.externalId}</code>) est
          relié au bot @{gateway.workspaceName} depuis le {formatDate(linked.linkedAt)}. La
          conversation avec l’agent dans Telegram arrive avec un prochain lot.
        </p>
      ) : !gateway.available ? (
        <p className="text-sm text-muted">La passerelle est indisponible pour l’instant.</p>
      ) : pending && claimedBy !== null ? (
        <p className="text-sm text-muted">
          Le compte Telegram <strong>{claimedBy}</strong> (identifiant{' '}
          <code>{pending.claimedId}</code>) a accepté la liaison. Confirmez seulement si c’est le
          vôtre, avant {formatTime(pending.expiresAt)} : il sera alors relié à votre compte.
        </p>
      ) : pending ? (
        <div className="flex flex-col gap-2 text-sm text-muted">
          {shown ? (
            <p>
              <a
                href={shown.link}
                target="_blank"
                rel="noopener noreferrer"
                className="inline-flex items-center gap-1 font-medium text-accent underline"
              >
                Ouvrir le bot @{gateway.workspaceName} dans Telegram
                <ExternalLink className="size-3.5" aria-hidden />
              </a>
            </p>
          ) : (
            <p>Vous n’avez plus le lien ? Demandez-en un nouveau.</p>
          )}
          <p>
            Dans Telegram, démarrez le bot : il vous dira à quel compte ce lien relie le vôtre.
            Répondez <code>/oui</code> si c’est le vôtre, puis revenez ici confirmer. Lien valable
            jusqu’à {formatTime(pending.expiresAt)}.
          </p>
        </div>
      ) : (
        <p className="text-sm text-muted">
          Pour relier votre compte Telegram, demandez le lien du bot @{gateway.workspaceName} et
          ouvrez-le dans Telegram.
        </p>
      )}
      {linked && !gateway.available && (
        <p className="text-sm text-muted">
          La passerelle est désactivée, ou vous n’utilisez plus l’agent. Vous pouvez retirer la
          liaison.
        </p>
      )}
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex flex-wrap gap-2">
        {linked && (
          <Button variant="danger" loading={loading('unlink')} onClick={unlink}>
            Retirer la liaison
          </Button>
        )}
        {!linked && gateway.available && pending && claimedBy !== null && (
          <>
            <Button
              disabled={held('confirm')}
              loading={loading('confirm')}
              onClick={() => run('confirm', () => api.post(`${path}/telegram-link`))}
            >
              Confirmer la liaison
            </Button>
            <Button
              variant="secondary"
              disabled={held('refuse')}
              loading={loading('refuse')}
              onClick={() => run('refuse', () => api.delete(`${path}/telegram-code`))}
            >
              Ce n’est pas moi
            </Button>
          </>
        )}
        {!linked && gateway.available && pending && claimedBy === null && (
          <>
            {!shown && (
              <Button disabled={held('ask')} loading={loading('ask')} onClick={ask}>
                Demander un nouveau lien
              </Button>
            )}
            <Button
              variant="secondary"
              disabled={held('cancel')}
              loading={loading('cancel')}
              onClick={() => run('cancel', () => api.delete(`${path}/telegram-code`))}
            >
              Annuler la liaison
            </Button>
          </>
        )}
        {!linked && gateway.available && !pending && (
          <Button loading={loading('ask')} onClick={ask}>
            Relier mon compte Telegram
          </Button>
        )}
      </div>
    </section>
  );
}
