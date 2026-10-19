'use client';

import { X } from 'lucide-react';
import { Button, ErrorNote, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type OrgUnitView,
  type DirectoryUser,
  type Roles,
} from '@/lib/api';
import { formatDate } from '@/lib/format';
import { GrantRoleForm } from './grant-role-form';

/** Roles held by an account, their revocation and new grants (`role:manage`). */
export function AssignmentsEditor({
  user,
  units,
  onChanged,
}: {
  user: DirectoryUser;
  units: OrgUnitView[];
  onChanged: () => void;
}) {
  const roles = useApiData<Roles>('/admin/roles');
  const revoke = useAction();

  async function revokeAssignment(id: string, label: string) {
    if (!window.confirm(`Retirer l’habilitation « ${label} » ?`)) return;
    if (await revoke.run(() => api.delete(`/admin/assignments/${id}`))) onChanged();
  }

  return (
    <div className="flex flex-col gap-3">
      <ul className="flex flex-col gap-1">
        {(user.assignments ?? []).map((a) => (
          <li key={a.id} className="flex items-center gap-2 text-sm">
            <span className="flex-1">
              {a.label}
              {a.expiresAt && (
                <span className="text-xs text-muted"> — jusqu’au {formatDate(a.expiresAt)}</span>
              )}
            </span>
            <Button
              variant="ghost"
              className="px-2 py-1"
              onClick={() => revokeAssignment(a.id, a.label)}
              loading={revoke.pending}
            >
              <X className="size-3.5" aria-hidden />
              <span className="sr-only">Retirer {a.label}</span>
            </Button>
          </li>
        ))}
      </ul>
      {revoke.error && <ErrorNote message={revoke.error} />}
      <Loaded state={roles}>
        {(data) => (
          <GrantRoleForm userId={user.id} roles={data.roles} units={units} onGranted={onChanged} />
        )}
      </Loaded>
    </div>
  );
}
