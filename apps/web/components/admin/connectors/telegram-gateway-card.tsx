'use client';

import { useState } from 'react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import type { GatewayView } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { TelegramGatewayForm } from './telegram-gateway-form';
import { useGatewayActions } from './use-gateway-actions';

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
  const [editing, setEditing] = useState(false);
  const { action, loading, held, toggle, remove } = useGatewayActions(gateway, onChanged);

  function done(changed: boolean) {
    setEditing(false);
    if (changed) onChanged();
  }

  return (
    <Card className="flex flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 font-medium">{gateway.name}</p>
        <Badge>Telegram</Badge>
        <Badge tone={gateway.enabled ? 'ok' : 'neutral'}>
          {gateway.enabled ? 'Active' : 'Désactivée'}
        </Badge>
      </div>
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
      <p className="text-xs text-muted">Modifié le {formatDate(gateway.updatedAt)}</p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex flex-wrap gap-2">
        {!editing && (
          <Button variant="secondary" onClick={() => setEditing(true)}>
            Modifier
          </Button>
        )}
        <Button
          variant="secondary"
          disabled={held('toggle')}
          loading={loading('toggle')}
          onClick={toggle}
        >
          {gateway.enabled ? 'Désactiver' : 'Activer'}
        </Button>
        <Button
          variant="danger"
          disabled={held('remove')}
          loading={loading('remove')}
          onClick={() =>
            remove(
              `Supprimer le bot « ${gateway.name} », son jeton, son webhook et les ${gateway.identities} compte(s) Telegram que les collaborateurs y ont relié(s) ?`,
            )
          }
        >
          Supprimer
        </Button>
      </div>
      {editing && <TelegramGatewayForm gateway={gateway} onDone={done} />}
    </Card>
  );
}
