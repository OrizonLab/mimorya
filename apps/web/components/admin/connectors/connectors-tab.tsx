'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Badge, Button, Card, EmptyState, Loaded } from '@/components/ui';
import { useApiData, type Connectors } from '@/lib/api';
import { ConnectorCard } from './connector-card';
import { McpForm } from './mcp-form';

/** The company's connectors, and adding an MCP server. */
export function ConnectorsTab() {
  const state = useApiData<Connectors>('/admin/connectors');
  const [adding, setAdding] = useState(false);
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-4">
          {adding ? (
            <Card className="p-4">
              <p className="mb-3 font-medium">Nouveau serveur MCP</p>
              <McpForm
                data={data}
                onDone={(changed) => {
                  setAdding(false);
                  if (changed) state.reload();
                }}
              />
            </Card>
          ) : (
            <Button variant="secondary" className="w-fit" onClick={() => setAdding(true)}>
              <Plus className="size-4" aria-hidden /> Ajouter un serveur MCP
            </Button>
          )}
          {data.instances.length === 0 && <EmptyState title="Aucun connecteur" />}
          {data.instances.map((connector) => (
            <ConnectorCard
              key={connector.id}
              connector={connector}
              data={data}
              onChanged={state.reload}
            />
          ))}
          <section>
            <h2 className="mb-2 text-sm font-semibold">Types de connecteurs</h2>
            <div className="grid gap-3 md:grid-cols-2">
              {data.types.map((t) => (
                <Card key={t.type} className="p-4">
                  <div className="flex items-center gap-2">
                    <p className="font-medium">{t.name}</p>
                    {t.demo && <Badge tone="warn">Démonstration</Badge>}
                  </div>
                  <p className="mt-1 text-sm text-muted">{t.description}</p>
                </Card>
              ))}
            </div>
          </section>
          <p className="text-xs text-muted">
            Les réponses des serveurs MCP sont traitées comme des données non fiables : elles ne
            changent ni les permissions ni les règles de l’agent.
          </p>
        </div>
      )}
    </Loaded>
  );
}
