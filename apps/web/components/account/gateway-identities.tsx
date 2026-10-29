'use client';

import { Card, Loaded } from '@/components/ui';
import { useApiData, type AccountGateways } from '@/lib/api';
import { SlackIdentityRow } from './slack-identity-row';
import { TelegramIdentityRow } from './telegram-identity-row';

/**
 * Who the signed-in person is on their company's conversation gateways
 * (ADR-125, ADR-126, ADR-134): their Slack user, linked from the link the
 * application sent them in Slack; their Telegram account, linked from here
 * with the bot's consent asked in Telegram. Shown once the company declared
 * a gateway.
 */
export function GatewayIdentities() {
  const state = useApiData<AccountGateways>('/account/gateways');
  return (
    <Loaded state={state}>
      {(data) =>
        data.gateways.length === 0 ? null : (
          <Card className="flex max-w-2xl flex-col gap-3 p-4">
            <h2 className="font-medium">Messageries reliées</h2>
            {data.gateways.map((gateway) =>
              gateway.type === 'telegram' ? (
                <TelegramIdentityRow
                  key={gateway.gatewayId}
                  gateway={gateway}
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
        )
      }
    </Loaded>
  );
}
