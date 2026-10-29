'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Button, Card, Loaded } from '@/components/ui';
import { useApiData, type Gateways } from '@/lib/api';
import { GatewayCard } from './gateway-card';
import { SlackGatewayForm } from './slack-gateway-form';

/**
 * The company's conversation gateways (ADR-125, ADR-126): its Slack
 * application, one per company, declared here. A gateway brings the agent
 * no tool: it is where people talk with it.
 */
export function GatewaysSection() {
  const state = useApiData<Gateways>('/admin/gateways');
  const [adding, setAdding] = useState(false);
  const done = (changed: boolean) => {
    setAdding(false);
    if (changed) state.reload();
  };
  return (
    <section aria-label="Passerelles de conversation" className="flex flex-col gap-3">
      <h2 className="text-sm font-semibold">Passerelles de conversation</h2>
      <Loaded state={state}>
        {(data) => (
          <div className="flex flex-col gap-3">
            {adding && (
              <Card className="p-4">
                <p className="mb-3 font-medium">Application Slack de l’entreprise</p>
                <SlackGatewayForm onDone={done} />
              </Card>
            )}
            {!adding && !data.gateways.some((g) => g.type === 'slack') && (
              <div>
                <Button variant="secondary" onClick={() => setAdding(true)}>
                  <Plus className="size-4" aria-hidden /> Déclarer l’application Slack
                </Button>
              </div>
            )}
            {/* A Telegram bot's own card comes with its next lot (ADR-133). */}
            {data.gateways
              .filter((gateway) => gateway.type === 'slack')
              .map((gateway) => (
                <GatewayCard
                  key={gateway.id}
                  gateway={gateway}
                  scopes={data.slackScopes}
                  events={data.slackEvents}
                  onChanged={state.reload}
                />
              ))}
          </div>
        )}
      </Loaded>
    </section>
  );
}
