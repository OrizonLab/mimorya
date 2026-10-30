'use client';

import { useState } from 'react';
import { Card, Loaded } from '@/components/ui';
import { useApiData, type AccountGateways, type TelegramLink } from '@/lib/api';
import { useRefreshWhile } from '@/lib/use-refresh-while';
import { SlackIdentityRow } from './slack-identity-row';
import { TelegramIdentityRow } from './telegram-identity-row';

/** How long after a read the list is read again while a bot waits for consent. */
const WAITING_REFRESH_MS = 3000;

/**
 * Who the signed-in person is on their company's conversation gateways
 * (ADR-125, ADR-126, ADR-134): their Slack user, linked from the link the
 * application sent them in Slack; their Telegram account, linked from here
 * with the bot's consent asked in Telegram. Shown once the company declared
 * a gateway. While a bot waits for their consent, the list is read again
 * three seconds after each read ends (ADR-105); the bot's link, shown
 * once, is kept here, so that a failed read does not lose it.
 */
export function GatewayIdentities() {
  const state = useApiData<AccountGateways>('/account/gateways');
  const [links, setLinks] = useState<Record<string, TelegramLink | null>>({});
  const waiting = (state.data?.gateways ?? []).some(
    (g) => g.available && g.pending !== null && g.pending.claimedBy === null,
  );
  useRefreshWhile(waiting && !state.loading, state.reload, WAITING_REFRESH_MS);
  return (
    <Loaded state={state}>
      {(data) => {
        // A Teams bot's own rows come with its next lot (ADR-138).
        const shown = data.gateways.filter((gateway) => gateway.type !== 'teams');
        return shown.length === 0 ? null : (
          <Card className="flex max-w-2xl flex-col gap-3 p-4">
            <h2 className="font-medium">Messageries reliées</h2>
            {shown.map((gateway) =>
              gateway.type === 'telegram' ? (
                <TelegramIdentityRow
                  key={gateway.gatewayId}
                  gateway={gateway}
                  link={links[gateway.gatewayId] ?? null}
                  onLink={(link) => setLinks((kept) => ({ ...kept, [gateway.gatewayId]: link }))}
                  onChanged={state.reload}
                />
              ) : (
                <SlackIdentityRow
                  key={gateway.gatewayId}
                  gateway={gateway}
                  onChanged={state.reload}
                />
              ),
            )}
          </Card>
        );
      }}
    </Loaded>
  );
}
