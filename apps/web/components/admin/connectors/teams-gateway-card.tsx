'use client';

import type { GatewayView } from '@/lib/api';
import { GatewayCardFrame } from './gateway-card-frame';
import { TeamsGatewayForm } from './teams-gateway-form';

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
  return (
    <GatewayCardFrame
      gateway={gateway}
      product="Teams"
      removal={`Supprimer le bot « ${gateway.name} », son secret client et les ${gateway.identities} compte(s) Teams que les collaborateurs y ont relié(s) ?`}
      form={(done) => <TeamsGatewayForm gateway={gateway} onDone={done} />}
      onChanged={onChanged}
    >
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
    </GatewayCardFrame>
  );
}
