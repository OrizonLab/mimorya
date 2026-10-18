'use client';

import { Badge, Card, Loaded } from '@/components/ui';
import { useApiData } from '@/lib/api';

interface ConnectorsView {
  instances: Array<{
    id: string;
    type: string;
    name: string;
    enabled: boolean;
    status: string;
    updatedAt: string;
  }>;
  types: Array<{ type: string; name: string; description: string; demo: boolean }>;
}

export function ConnectorsTab() {
  const state = useApiData<ConnectorsView>('/admin/connectors');
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="flex flex-col gap-6">
          <section>
            <h2 className="mb-2 text-sm font-semibold">Connecteurs actifs</h2>
            <Card className="divide-y divide-line">
              {data.instances.map((i) => (
                <div key={i.id} className="flex flex-wrap items-center gap-2 px-4 py-3 text-sm">
                  <span className="flex-1 font-medium">{i.name}</span>
                  <Badge>{i.type}</Badge>
                  <Badge tone={i.enabled ? 'ok' : 'neutral'}>
                    {i.enabled ? 'Activé' : 'Désactivé'}
                  </Badge>
                </div>
              ))}
              {data.instances.length === 0 && (
                <p className="px-4 py-3 text-sm text-muted">Aucun connecteur.</p>
              )}
            </Card>
          </section>
          <section>
            <h2 className="mb-2 text-sm font-semibold">Catalogue</h2>
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
        </div>
      )}
    </Loaded>
  );
}
