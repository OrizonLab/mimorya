'use client';

import { useState } from 'react';
import { DataTable, Loaded } from '@/components/ui';
import { useApiData, type Organization, type Directory } from '@/lib/api';
import { useMe } from '@/lib/me-context';
import { AccountNoticeCard } from './account-notice';
import { NewUserForm } from './new-user-form';
import { ScimTokens } from './scim-tokens';
import type { AccountNotice } from './types';
import { UserRow } from './user-row';

/**
 * The « Utilisateurs » tab: the company's accounts, and for who manages
 * them company-wide, the making of an account and the tokens of the
 * identity provider that provisions them through SCIM (ADR-183).
 */
export function UsersTab() {
  const me = useMe();
  const directory = useApiData<Directory>('/admin/users');
  const organization = useApiData<Organization>('/admin/organization');
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
              {data.canManage.users && <ScimTokens />}
            </>
          )}
        </Loaded>
      )}
    </Loaded>
  );
}
