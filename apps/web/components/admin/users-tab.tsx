'use client';

import { Cell, DataTable, Loaded } from '@/components/ui';
import { useApiData } from '@/lib/api';
import { formatDate } from '@/lib/format';

interface UserView {
  id: string;
  displayName: string;
  jobTitle: string | null;
  orgUnit: string | null;
  email?: string;
  lastLoginAt?: string | null;
  grants?: string[];
}

export function UsersTab() {
  const state = useApiData<{ users: UserView[] }>('/admin/users');
  return (
    <Loaded state={state}>
      {(data) => (
        <DataTable columns={['Collaborateur', 'Unité', 'Habilitations', 'Dernière connexion']}>
          {data.users.map((u) => (
            <tr key={u.id} className="align-top">
              <Cell>
                <p className="font-medium">{u.displayName}</p>
                <p className="text-xs text-muted">{u.jobTitle}</p>
                {u.email && <p className="text-xs text-muted">{u.email}</p>}
              </Cell>
              <Cell className="text-muted">{u.orgUnit ?? '—'}</Cell>
              <Cell>
                <ul className="flex flex-col gap-1">
                  {(u.grants ?? []).map((g) => (
                    <li key={g} className="text-xs">
                      {g}
                    </li>
                  ))}
                </ul>
              </Cell>
              <Cell className="text-xs whitespace-nowrap text-muted">
                {formatDate(u.lastLoginAt)}
              </Cell>
            </tr>
          ))}
        </DataTable>
      )}
    </Loaded>
  );
}
