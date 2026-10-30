'use client';

import type { GatewayView } from '@/lib/api';
import { GatewayCardFrame } from './gateway-card-frame';
import { TelegramGatewayForm } from './telegram-gateway-form';

/**
 * The company's Telegram bot (ADR-133, ADR-134): the bot Telegram said its
 * token is, the address the platform told Telegram to send its updates to,
 * how many people linked their Telegram account; changed, switched on or
 * off, and deleted with those links, Telegram told to send its updates
 * nowhere.
 */
export function TelegramGatewayCard({
  gateway,
  onChanged,
}: {
  gateway: GatewayView;
  onChanged: () => void;
}) {
  return (
    <GatewayCardFrame
      gateway={gateway}
      product="Telegram"
      removal={`Supprimer le bot « ${gateway.name} », son jeton, son webhook et les ${gateway.identities} compte(s) Telegram que les collaborateurs y ont relié(s) ?`}
      form={(done) => <TelegramGatewayForm gateway={gateway} onDone={done} />}
      onChanged={onChanged}
    >
      <div className="flex flex-col gap-1 text-sm text-muted">
        <p>
          Bot @{gateway.workspaceName} (<code>{gateway.workspaceId}</code>) ·{' '}
          {gateway.secretsStored
            ? 'jeton et secret du webhook enregistrés'
            : 'jeton ou secret du webhook absent'}{' '}
          · {gateway.identities} compte(s) relié(s)
        </p>
        <p>
          Telegram envoie les messages du bot à <code>{gateway.eventsUrl}</code>, adresse que la
          plateforme lui a donnée avec son secret : rien n’est à régler dans Telegram.
        </p>
      </div>
    </GatewayCardFrame>
  );
}
