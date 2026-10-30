'use client';

import type { GatewayView } from '@/lib/api';
import { GatewayCardFrame } from './gateway-card-frame';
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
  return (
    <GatewayCardFrame
      gateway={gateway}
      product="Slack"
      check="Vérifier auprès de Slack"
      removal={`Supprimer la passerelle « ${gateway.name} », son jeton, son secret et les ${gateway.identities} compte(s) Slack que les collaborateurs y ont relié(s) ?`}
      form={(done) => <SlackGatewayForm gateway={gateway} onDone={done} />}
      onChanged={onChanged}
    >
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
    </GatewayCardFrame>
  );
}
