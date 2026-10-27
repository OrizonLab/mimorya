'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Badge, Button, Card, EmptyState, Loaded } from '@/components/ui';
import { useApiData, type Connectors } from '@/lib/api';
import { ConnectorCard } from './connector-card';
import { McpForm } from './mcp-form';
import { RestForm } from './rest-form';

/** The company's connectors, and adding an MCP server or a REST API. */
export function ConnectorsTab() {
  const state = useApiData<Connectors>('/admin/connectors');
  // What is being added, if anything.
  const [adding, setAdding] = useState<'mcp' | 'rest' | null>(null);
  const done = (changed: boolean) => {
    setAdding(null);
    if (changed) state.reload();
  };
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-4">
          {adding === 'mcp' && (
            <Card className="p-4">
              <p className="mb-3 font-medium">Nouveau serveur MCP</p>
              <McpForm data={data} onDone={done} />
            </Card>
          )}
          {adding === 'rest' && (
            <Card className="p-4">
              <p className="mb-3 font-medium">Nouvelle API REST</p>
              <RestForm data={data} onDone={done} />
            </Card>
          )}
          {!adding && (
            <div className="flex flex-wrap gap-2">
              <Button variant="secondary" onClick={() => setAdding('mcp')}>
                <Plus className="size-4" aria-hidden /> Ajouter un serveur MCP
              </Button>
              <Button variant="secondary" onClick={() => setAdding('rest')}>
                <Plus className="size-4" aria-hidden /> Ajouter une API REST
              </Button>
            </div>
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
            Les réponses des serveurs MCP et des API REST sont traitées comme des données non
            fiables : elles ne changent ni les permissions ni les règles de l’agent.
          </p>
        </div>
      )}
    </Loaded>
  );
}
