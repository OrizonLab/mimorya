'use client';

import { useState } from 'react';
import { DataTable, Loaded } from '@/components/ui';
import { useApiData } from '@/lib/api';
import { useMe } from '@/lib/me-context';
import type { OrganizationView } from '../organization/types';
import { NewUserForm } from './new-user-form';
import { TemporaryPassword } from './temporary-password';
import type { DirectoryView } from './types';
import { UserRow } from './user-row';

export function UsersTab() {
  const me = useMe();
  const directory = useApiData<DirectoryView>('/admin/users');
  const organization = useApiData<OrganizationView>('/admin/organization');
  const [secret, setSecret] = useState<{ who: string; password: string } | null>(null);

  return (
    <Loaded state={directory}>
      {(data) => (
        <Loaded state={organization}>
          {(org) => (
            <>
              {secret && (
                <TemporaryPassword
                  who={secret.who}
                  password={secret.password}
                  onClose={() => setSecret(null)}
                />
              )}
              {data.canManage.users && (
                <NewUserForm
                  units={org.units}
                  onCreated={(who, password) => {
                    setSecret({ who, password });
                    directory.reload();
                  }}
                />
              )}
              <DataTable
                columns={['Collaborateur', 'Unité', 'Habilitations', 'Dernière connexion']}
              >
                {data.users.map((u) => (
                  <UserRow
                    key={u.id}
                    user={u}
                    units={org.units}
                    canManage={data.canManage}
                    isSelf={u.id === me.user.id}
                    onChanged={directory.reload}
                    onPassword={(who, password) => setSecret({ who, password })}
                  />
                ))}
              </DataTable>
            </>
          )}
        </Loaded>
      )}
    </Loaded>
  );
}
