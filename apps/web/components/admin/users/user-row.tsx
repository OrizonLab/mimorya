'use client';

import { useState } from 'react';
import { Button, Cell, LabelBadge } from '@/components/ui';
import { formatDate } from '@/lib/format';
import { ACCOUNT_STATUS } from '@/lib/labels';
import type { OrgUnitView } from '../organization/types';
import { AssignmentsEditor } from './assignments-editor';
import { AttributesEditor } from './attributes-editor';
import { ProfileEditor } from './profile-editor';
import type { DirectoryView, UserView } from './types';

/** A directory row; administrators open it to manage the account. */
export function UserRow({
  user,
  units,
  canManage,
  isSelf,
  onChanged,
  onPassword,
}: {
  user: UserView;
  units: OrgUnitView[];
  canManage: DirectoryView['canManage'];
  isSelf: boolean;
  onChanged: () => void;
  onPassword: (displayName: string, temporaryPassword: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const manageable = (canManage.users || canManage.roles) && !isSelf;
  return (
    <>
      <tr className="align-top">
        <Cell>
          <p className="font-medium">{user.displayName}</p>
          <p className="text-xs text-muted">{user.jobTitle}</p>
          {user.email && <p className="text-xs text-muted">{user.email}</p>}
          {user.status !== 'active' && <LabelBadge labels={ACCOUNT_STATUS} value={user.status} />}
        </Cell>
        <Cell className="text-muted">{user.orgUnit ?? '—'}</Cell>
        <Cell>
          <ul className="flex flex-col gap-1">
            {(user.assignments ?? []).map((a) => (
              <li key={a.id} className="text-xs">
                {a.label}
              </li>
            ))}
          </ul>
        </Cell>
        <Cell className="text-xs whitespace-nowrap text-muted">
          {formatDate(user.lastLoginAt)}
          {manageable && (
            <Button
              variant="secondary"
              className="mt-2 block px-2 py-1 text-xs"
              onClick={() => setOpen(!open)}
            >
              {open ? 'Fermer' : 'Gérer'}
            </Button>
          )}
          {isSelf && canManage.users && (
            <p className="mt-2 whitespace-normal">
              Votre compte : géré par un autre administrateur.
            </p>
          )}
        </Cell>
      </tr>
      {open && (
        <tr>
          <td colSpan={4} className="bg-surface-2/50 px-4 py-4">
            <div className="flex flex-col gap-6">
              {canManage.users && (
                <ProfileEditor
                  user={user}
                  units={units}
                  onChanged={onChanged}
                  onPassword={(password) => onPassword(user.displayName, password)}
                />
              )}
              {canManage.users && (
                <section>
                  <h3 className="mb-2 text-sm font-medium">Attributs</h3>
                  <AttributesEditor user={user} onChanged={onChanged} />
                </section>
              )}
              {canManage.roles && (
                <section>
                  <h3 className="mb-2 text-sm font-medium">Habilitations</h3>
                  <AssignmentsEditor user={user} units={units} onChanged={onChanged} />
                </section>
              )}
            </div>
          </td>
        </tr>
      )}
    </>
  );
}
