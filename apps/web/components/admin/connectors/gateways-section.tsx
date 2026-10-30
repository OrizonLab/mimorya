'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Button, Card, Loaded } from '@/components/ui';
import { useApiData, type Gateways } from '@/lib/api';
import { GatewayCard } from './gateway-card';
import { SlackGatewayForm } from './slack-gateway-form';
import { TeamsGatewayCard } from './teams-gateway-card';
import { TeamsGatewayForm } from './teams-gateway-form';
import { TelegramGatewayCard } from './telegram-gateway-card';
import { TelegramGatewayForm } from './telegram-gateway-form';

/** The services a company declares a gateway of, one each. */
type Service = 'slack' | 'telegram' | 'teams';
const SERVICES: Service[] = ['slack', 'telegram', 'teams'];

/**
 * The company's conversation gateways (ADR-125, ADR-126, ADR-134,
 * ADR-139): its Slack application, its Telegram bot and its Teams bot, one
 * of each per company, declared here. A gateway brings the agent no tool:
 * it is where people talk with it.
 */
export function GatewaysSection() {
  const state = useApiData<Gateways>('/admin/gateways');
  const [adding, setAdding] = useState<Service | null>(null);
  const done = (changed: boolean) => {
    setAdding(null);
    if (changed) state.reload();
  };
  return (
    <section aria-label="Passerelles de conversation" className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold">Passerelles de conversation</h2>
      <Loaded state={state}>
        {(data) => {
          const declared = (service: Service) => data.gateways.some((g) => g.type === service);
          return (
            <div className="flex flex-col gap-3">
              {adding === 'slack' && (
                <Card className="p-4">
                  <p className="mb-3 font-medium">Application Slack de l’entreprise</p>
                  <SlackGatewayForm onDone={done} />
                </Card>
              )}
              {adding === 'telegram' && (
                <Card className="p-4">
                  <p className="mb-3 font-medium">Bot Telegram de l’entreprise</p>
                  <TelegramGatewayForm onDone={done} />
                </Card>
              )}
              {adding === 'teams' && (
                <Card className="p-4">
                  <p className="mb-3 font-medium">Bot Teams de l’entreprise</p>
                  <TeamsGatewayForm onDone={done} />
                </Card>
              )}
              {adding === null && SERVICES.some((service) => !declared(service)) && (
                <div className="flex flex-wrap gap-2">
                  {!declared('slack') && (
                    <Button variant="secondary" onClick={() => setAdding('slack')}>
                      <Plus className="size-4" aria-hidden /> Déclarer l’application Slack
                    </Button>
                  )}
                  {!declared('telegram') && (
                    <Button variant="secondary" onClick={() => setAdding('telegram')}>
                      <Plus className="size-4" aria-hidden /> Déclarer le bot Telegram
                    </Button>
                  )}
                  {!declared('teams') && (
                    <Button variant="secondary" onClick={() => setAdding('teams')}>
                      <Plus className="size-4" aria-hidden /> Déclarer le bot Teams
                    </Button>
                  )}
                </div>
              )}
              {data.gateways.map((gateway) =>
                gateway.type === 'telegram' ? (
                  <TelegramGatewayCard
                    key={gateway.id}
                    gateway={gateway}
                    onChanged={state.reload}
                  />
                ) : gateway.type === 'teams' ? (
                  <TeamsGatewayCard key={gateway.id} gateway={gateway} onChanged={state.reload} />
                ) : (
                  <GatewayCard
                    key={gateway.id}
                    gateway={gateway}
                    scopes={data.slackScopes}
                    events={data.slackEvents}
                    onChanged={state.reload}
                  />
                ),
              )}
            </div>
          );
        }}
      </Loaded>
    </section>
  );
}
