'use client';

import type { ReactNode } from 'react';
import { Badge, Button, ErrorNote } from '@/components/ui';
import { api, useAction, type AccountGatewayView, type GatewayIdentity } from '@/lib/api';

/**
 * The signed-in person's identity on a gateway whose application or bot
 * sends them the link to their page when they write to it (Slack, Teams;
 * ADR-126, ADR-139): what is linked (`linked`), or how to link it
 * (`howTo`), the gateway said unavailable otherwise; the link removed here
 * once `unlinkQuestion` is confirmed.
 */
export function LinkedByMessageRow({
  gateway,
  linked,
  howTo,
  unlinkQuestion,
  onChanged,
}: {
  gateway: AccountGatewayView;
  linked: (identity: GatewayIdentity) => ReactNode;
  howTo: ReactNode;
  unlinkQuestion: string;
  onChanged: () => void;
}) {
  const action = useAction();
  const identity = gateway.linked;

  async function unlink() {
    if (!window.confirm(unlinkQuestion)) return;
    if (await action.run(() => api.delete(`/account/gateways/${gateway.gatewayId}`))) onChanged();
  }

  return (
    <section aria-label={gateway.name} className="flex flex-col gap-2 border-t border-line pt-3">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 text-sm font-medium">{gateway.name}</p>
        <Badge tone={identity ? 'ok' : 'neutral'}>{identity ? 'Relié' : 'Non relié'}</Badge>
      </div>
      {identity ? (
        <p className="text-sm text-muted">{linked(identity)}</p>
      ) : gateway.available ? (
        <p className="text-sm text-muted">{howTo}</p>
      ) : (
        <p className="text-sm text-muted">La passerelle est indisponible pour l’instant.</p>
      )}
      {identity && !gateway.available && (
        <p className="text-sm text-muted">
          La passerelle est désactivée, ou vous n’utilisez plus l’agent. Vous pouvez retirer la
          liaison.
        </p>
      )}
      {action.error && <ErrorNote message={action.error} />}
      {identity && (
        <div>
          <Button variant="danger" loading={action.pending} onClick={unlink}>
            Retirer la liaison
          </Button>
        </div>
      )}
    </section>
  );
}
