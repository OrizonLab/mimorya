'use client';

import { useState } from 'react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type GatewayUpdate, type GatewayView } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { GatewayScopes } from './gateway-scopes';
import { SlackGatewayForm } from './slack-gateway-form';

/**
 * One conversation gateway (ADR-125, ADR-126): what Slack said of its
 * application when it was declared or last checked, its token's
 * permissions among it (ADR-132), the address to give Slack for its events,
 * the permissions and the event it asks for, how many people linked their
 * identity; checked again with Slack, changed, switched on or off, and
 * deleted with those identities.
 */
export function GatewayCard({
  gateway,
  scopes,
  events,
  onChanged,
}: {
  gateway: GatewayView;
  scopes: string[];
  events: string[];
  onChanged: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [pressed, setPressed] = useState<'check' | 'toggle' | 'remove' | null>(null);
  const action = useAction();
  const path = `/admin/gateways/${gateway.id}`;

  async function toggle() {
    const update = { enabled: !gateway.enabled } satisfies GatewayUpdate;
    setPressed('toggle');
    if (await action.run(() => api.patch(path, update))) onChanged();
  }

  async function check() {
    setPressed('check');
    if (await action.run(() => api.post(`${path}/check`))) onChanged();
  }

  async function remove() {
    const question = `Supprimer la passerelle « ${gateway.name} », son jeton, son secret et les ${gateway.identities} compte(s) Slack que les collaborateurs y ont relié(s) ?`;
    if (!window.confirm(question)) return;
    setPressed('remove');
    if (await action.run(() => api.delete(path))) onChanged();
  }

  function done(changed: boolean) {
    setEditing(false);
    if (changed) onChanged();
  }

  return (
    <Card className="flex flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 font-medium">{gateway.name}</p>
        <Badge>Slack</Badge>
        <Badge tone={gateway.enabled ? 'ok' : 'neutral'}>
          {gateway.enabled ? 'Active' : 'Désactivée'}
        </Badge>
      </div>
      <div className="flex flex-col gap-1 text-sm text-muted">
        <p>
          Espace de travail « {gateway.workspaceName} » (<code>{gateway.workspaceId}</code>) · bot{' '}
          <code>{gateway.botUserId}</code> ·{' '}
          {gateway.secretsStored ? 'jeton et secret enregistrés' : 'jeton ou secret absent'} ·{' '}
          {gateway.identities} compte(s) relié(s)
        </p>
        <p>
          Adresse à donner à Slack, pour les événements (« Event Subscriptions ») comme pour les
          boutons de validation (« Interactivity &amp; Shortcuts ») :{' '}
          <code>{gateway.eventsUrl}</code>
        </p>
        <p>
          Permissions du bot : {scopes.join(', ')} · événement : {events.join(', ')}
        </p>
      </div>
      <GatewayScopes gateway={gateway} />
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
          disabled={action.pending && pressed !== 'check'}
          loading={action.pending && pressed === 'check'}
          onClick={check}
        >
          Vérifier auprès de Slack
        </Button>
        <Button
          variant="secondary"
          disabled={action.pending && pressed !== 'toggle'}
          loading={action.pending && pressed === 'toggle'}
          onClick={toggle}
        >
          {gateway.enabled ? 'Désactiver' : 'Activer'}
        </Button>
        <Button
          variant="danger"
          disabled={action.pending && pressed !== 'remove'}
          loading={action.pending && pressed === 'remove'}
          onClick={remove}
        >
          Supprimer
        </Button>
      </div>
      {editing && <SlackGatewayForm gateway={gateway} onDone={done} />}
    </Card>
  );
}
