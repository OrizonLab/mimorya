'use client';

import { Plus } from 'lucide-react';
import { useState } from 'react';
import { Button, Card, Loaded } from '@/components/ui';
import { useApiData, type Roles } from '@/lib/api';
import { RoleCard } from './role-card';
import { RoleForm } from './role-form';

/** Platform roles (fixed) and the company's custom roles. */
export function RolesTab() {
  const state = useApiData<Roles>('/admin/roles');
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<string | null>(null);
  return (
    <Loaded state={state}>
      {(data) => (
        <div className="grid gap-4 md:grid-cols-2">
          <div className="md:col-span-2">
            {creating ? (
              <Card className="p-4">
                <p className="mb-3 font-medium">Nouveau rôle</p>
                <RoleForm
                  catalog={data.catalog}
                  onDone={(changed) => {
                    setCreating(false);
                    if (changed) state.reload();
                  }}
                />
              </Card>
            ) : (
              <Button variant="secondary" onClick={() => setCreating(true)}>
                <Plus className="size-4" aria-hidden /> Nouveau rôle
              </Button>
            )}
          </div>
          {data.roles.map((role) => (
            <RoleCard
              key={role.id}
              role={role}
              catalog={data.catalog}
              editing={editing === role.id}
              onEdit={(on) => setEditing(on ? role.id : null)}
              onChanged={state.reload}
            />
          ))}
          <p className="text-xs text-muted md:col-span-2">
            Les permissions en orange ne peuvent jamais être exercées par l’agent, quels que soient
            les droits de l’utilisateur. Les rôles de la plateforme ne se modifient pas ; un rôle
            que vous détenez ne peut être modifié que par un autre administrateur.
          </p>
        </div>
      )}
    </Loaded>
  );
}
