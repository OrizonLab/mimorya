'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Badge, Button, Card, EmptyState, Loaded } from '@/components/ui';
import { useApiData, type Connectors } from '@/lib/api';
import { ConnectorCard } from './connector-card';
import { GatewaysSection } from './gateways-section';
import { GoogleWorkspaceForm } from './google-workspace-form';
import { McpForm } from './mcp-form';
import { Microsoft365Form } from './microsoft365-form';
import { RestForm } from './rest-form';

/**
 * The company's connectors, and adding an MCP server, a REST API, its
 * Microsoft 365 application or its Google Workspace client, one of each
 * per company (ADR-118, ADR-124); then its conversation gateways (ADR-126).
 */
export function ConnectorsTab() {
  const state = useApiData<Connectors>('/admin/connectors');
  // What is being added, if anything.
  const [adding, setAdding] = useState<'mcp' | 'rest' | 'microsoft365' | 'google_workspace' | null>(
    null,
  );
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
          {adding === 'microsoft365' && (
            <Card className="p-4">
              <p className="mb-3 font-medium">Application Microsoft 365 de l’entreprise</p>
              <Microsoft365Form onDone={done} />
            </Card>
          )}
          {adding === 'google_workspace' && (
            <Card className="p-4">
              <p className="mb-3 font-medium">Client Google Workspace de l’entreprise</p>
              <GoogleWorkspaceForm onDone={done} />
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
              {!data.instances.some((i) => i.type === 'microsoft365') && (
                <Button variant="secondary" onClick={() => setAdding('microsoft365')}>
                  <Plus className="size-4" aria-hidden /> Déclarer l’application Microsoft 365
                </Button>
              )}
              {!data.instances.some((i) => i.type === 'google_workspace') && (
                <Button variant="secondary" onClick={() => setAdding('google_workspace')}>
                  <Plus className="size-4" aria-hidden /> Déclarer le client Google Workspace
                </Button>
              )}
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
          <GatewaysSection />
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
