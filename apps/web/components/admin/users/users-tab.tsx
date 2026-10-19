'use client';

import { useState } from 'react';
import { DataTable, Loaded } from '@/components/ui';
import { useApiData } from '@/lib/api';
import { useMe } from '@/lib/me-context';
import type { OrganizationView } from '../organization/types';
import { AccountNoticeCard } from './account-notice';
import { NewUserForm } from './new-user-form';
import type { AccountNotice, DirectoryView } from './types';
import { UserRow } from './user-row';

export function UsersTab() {
  const me = useMe();
  const directory = useApiData<DirectoryView>('/admin/users');
  const organization = useApiData<OrganizationView>('/admin/organization');
  const [notice, setNotice] = useState<AccountNotice | null>(null);

  return (
    <Loaded state={directory}>
      {(data) => (
        <Loaded state={organization}>
          {(org) => (
            <>
              {notice && <AccountNoticeCard notice={notice} onClose={() => setNotice(null)} />}
              {data.canManage.users && (
                <NewUserForm
                  units={org.units}
                  onCreated={(created) => {
                    setNotice(created);
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
                    emailEnabled={data.emailEnabled}
                    isSelf={u.id === me.user.id}
                    onChanged={directory.reload}
                    onNotice={(issued) => {
                      setNotice(issued);
                      directory.reload();
                    }}
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
