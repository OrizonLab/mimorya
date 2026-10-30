'use client';

import { useState } from 'react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import type { GatewayView } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { TeamsGatewayForm } from './teams-gateway-form';
import { useGatewayActions } from './use-gateway-actions';

/**
 * The company's Teams bot (ADR-138, ADR-139): its application and its
 * directory, whether its client secret is stored, the messaging endpoint
 * to give the Azure bot, how many people linked their Teams account;
 * changed, switched on or off, and deleted with those links.
 */
export function TeamsGatewayCard({
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
        <Badge>Teams</Badge>
        <Badge tone={gateway.enabled ? 'ok' : 'neutral'}>
          {gateway.enabled ? 'Active' : 'Désactivée'}
        </Badge>
      </div>
      <div className="flex flex-col gap-1 text-sm text-muted">
        <p>
          Application <code>{gateway.botUserId}</code> de l’annuaire{' '}
          <code>{gateway.workspaceId}</code> ·{' '}
          {gateway.secretsStored ? 'secret client enregistré' : 'secret client absent'} ·{' '}
          {gateway.identities} compte(s) relié(s)
        </p>
        <p>
          Point de terminaison de messagerie, à saisir dans le bot Azure (« Configuration ») :{' '}
          <code>{gateway.eventsUrl}</code>
        </p>
      </div>
      <p className="text-xs text-muted">Modifiée le {formatDate(gateway.updatedAt)}</p>
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
              `Supprimer le bot « ${gateway.name} », son secret client et les ${gateway.identities} compte(s) Teams que les collaborateurs y ont relié(s) ?`,
            )
          }
        >
          Supprimer
        </Button>
      </div>
      {editing && <TeamsGatewayForm gateway={gateway} onDone={done} />}
    </Card>
  );
}
