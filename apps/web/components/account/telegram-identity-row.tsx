'use client';

import { ExternalLink } from 'lucide-react';
import { useState } from 'react';
import { Badge, Button, ErrorNote } from '@/components/ui';
import { api, useAction, type AccountGatewayView, type TelegramLink } from '@/lib/api';
import { formatDate, formatTime } from '@/lib/format';
import { useRefreshWhile } from '@/lib/use-refresh-while';

/** How often the row is read again while the bot waits for their consent. */
const WAITING_REFRESH_MS = 3000;

/**
 * The signed-in person's Telegram account on the company's bot (ADR-133,
 * ADR-134): linked from here. They ask for the bot's link, open it in
 * Telegram, where the bot tells them whose account it links and they
 * accept it (`/oui`); back here, they see which Telegram account accepted
 * it, by its name and its identifier, and confirm, or refuse it. The link
 * itself is shown once, as the server gives it: lost, a new one is asked.
 */
export function TelegramIdentityRow({
  gateway,
  onChanged,
}: {
  gateway: AccountGatewayView;
  onChanged: () => void;
}) {
  const action = useAction();
  const [link, setLink] = useState<TelegramLink | null>(null);
  const path = `/account/gateways/${gateway.gatewayId}`;
  const { linked, pending } = gateway;
  const claimedBy = pending?.claimedBy ?? null;
  // Read again until Telegram says whom it came from.
  useRefreshWhile(pending !== null && claimedBy === null, onChanged, WAITING_REFRESH_MS);

  async function ask() {
    const asked = await action.run(() => api.post<TelegramLink>(`${path}/telegram-code`));
    if (asked) {
      setLink(asked);
      onChanged();
    }
  }

  async function settle(call: () => Promise<unknown>) {
    if (await action.run(call)) {
      setLink(null);
      onChanged();
    }
  }

  async function unlink() {
    const question = `Retirer la liaison de votre compte Telegram au bot @${gateway.workspaceName} ? Vous pourrez le relier de nouveau d’ici.`;
    if (!window.confirm(question)) return;
    await settle(() => api.delete(path));
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
          vôtre : il sera alors relié à votre compte.
        </p>
      ) : pending ? (
        <div className="flex flex-col gap-2 text-sm text-muted">
          {link ? (
            <p>
              <a
                href={link.link}
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
          <Button variant="danger" loading={action.pending} onClick={unlink}>
            Retirer la liaison
          </Button>
        )}
        {!linked && gateway.available && pending && claimedBy !== null && (
          <>
            <Button
              loading={action.pending}
              onClick={() => settle(() => api.post(`${path}/telegram-link`))}
            >
              Confirmer la liaison
            </Button>
            <Button
              variant="secondary"
              disabled={action.pending}
              onClick={() => settle(() => api.delete(`${path}/telegram-code`))}
            >
              Ce n’est pas moi
            </Button>
          </>
        )}
        {!linked && gateway.available && pending && claimedBy === null && (
          <>
            {!link && (
              <Button loading={action.pending} onClick={ask}>
                Demander un nouveau lien
              </Button>
            )}
            <Button
              variant="secondary"
              disabled={action.pending}
              onClick={() => settle(() => api.delete(`${path}/telegram-code`))}
            >
              Annuler la liaison
            </Button>
          </>
        )}
        {!linked && gateway.available && !pending && (
          <Button loading={action.pending} onClick={ask}>
            Relier mon compte Telegram
          </Button>
        )}
      </div>
    </section>
  );
}
