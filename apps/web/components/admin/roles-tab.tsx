'use client';

import { Badge, Card, Loaded } from '@/components/ui';
import { useApiData } from '@/lib/api';

interface RolesView {
  roles: Array<{
    id: string;
    name: string;
    description: string;
    isSystem: boolean;
    permissions: string[];
  }>;
  catalog: Array<{ permission: string; label: string; delegable: boolean }>;
}

export function RolesTab() {
  const state = useApiData<RolesView>('/admin/roles');
  return (
    <Loaded state={state}>
      {(data) => {
        const catalog = new Map(data.catalog.map((p) => [p.permission, p]));
        return (
          <div className="grid gap-4 md:grid-cols-2">
            {data.roles.map((r) => (
              <Card key={r.id} className="p-4">
                <div className="flex items-center gap-2">
                  <p className="font-medium">{r.name}</p>
                  {r.isSystem && <Badge>Rôle plateforme</Badge>}
                </div>
                <p className="mt-1 text-xs text-muted">{r.description}</p>
                <ul className="mt-3 flex flex-wrap gap-1">
                  {r.permissions.map((p) => (
                    <li key={p}>
                      <Badge tone={catalog.get(p)?.delegable === false ? 'warn' : 'neutral'}>
                        {catalog.get(p)?.label ?? p}
                      </Badge>
                    </li>
                  ))}
                </ul>
              </Card>
            ))}
            <p className="text-xs text-muted md:col-span-2">
              Les permissions en orange ne peuvent jamais être exercées par l’agent, quels que
              soient les droits de l’utilisateur.
            </p>
          </div>
        );
      }}
    </Loaded>
  );
}
