'use client';

import { useState, type ReactNode } from 'react';
import { Card, Loaded } from '@/components/ui';
import { useApiData } from '@/lib/api';
import type { OrganizationView } from './types';
import { UnitNode, type Editing } from './unit-node';

export function OrganizationTab() {
  const state = useApiData<OrganizationView>('/admin/organization');
  const [editing, setEditing] = useState<Editing>(null);
  return (
    <Loaded state={state}>
      {(data) => (
        <Card className="p-4">
          <ul className="text-sm">
            <UnitTree
              data={data}
              parentId={null}
              depth={0}
              editing={editing}
              onEdit={setEditing}
              onChanged={state.reload}
            />
          </ul>
          <p className="mt-4 text-xs text-muted">
            Le responsable d’une unité est une information organisationnelle : il n’accorde aucun
            droit d’accès. Les accès proviennent uniquement des rôles attribués sur un périmètre.
            {data.canManage &&
              ' Une unité qui a encore des membres, des habilitations ou de la mémoire ne peut pas être supprimée.'}
          </p>
        </Card>
      )}
    </Loaded>
  );
}

function UnitTree({
  data,
  parentId,
  depth,
  editing,
  onEdit,
  onChanged,
}: {
  data: OrganizationView;
  parentId: string | null;
  depth: number;
  editing: Editing;
  onEdit: (editing: Editing) => void;
  onChanged: () => void;
}): ReactNode {
  return data.units
    .filter((u) => u.parentId === parentId)
    .map((unit) => (
      <UnitNode
        key={unit.id}
        data={data}
        unit={unit}
        depth={depth}
        editing={editing}
        onEdit={onEdit}
        onChanged={onChanged}
      >
        <UnitTree
          data={data}
          parentId={unit.id}
          depth={depth + 1}
          editing={editing}
          onEdit={onEdit}
          onChanged={onChanged}
        />
      </UnitNode>
    ));
}
