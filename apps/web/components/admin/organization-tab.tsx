'use client';

import type { ReactNode } from 'react';
import { Badge, Card, Loaded } from '@/components/ui';
import { useApiData } from '@/lib/api';

interface OrgUnitView {
  id: string;
  parentId: string | null;
  name: string;
  type: string;
  leadUserId: string | null;
  members: string[];
}

interface OrganizationView {
  units: OrgUnitView[];
  users: Array<{ id: string; displayName: string; jobTitle: string | null }>;
}

const UNIT_TYPES: Record<string, string> = {
  company: 'Entreprise',
  site: 'Établissement',
  direction: 'Direction',
  department: 'Service',
  service: 'Service',
  team: 'Équipe',
};

export function OrganizationTab() {
  const state = useApiData<OrganizationView>('/admin/organization');
  return (
    <Loaded state={state}>
      {(data) => (
        <Card className="p-4">
          <ul className="text-sm">
            <UnitTree data={data} parentId={null} depth={0} />
          </ul>
          <p className="mt-4 text-xs text-muted">
            Le responsable d’une unité est une information organisationnelle : il n’accorde aucun
            droit d’accès. Les accès proviennent uniquement des rôles attribués sur un périmètre.
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
}: {
  data: OrganizationView;
  parentId: string | null;
  depth: number;
}): ReactNode {
  const name = (userId: string) => data.users.find((u) => u.id === userId)?.displayName;
  return data.units
    .filter((u) => u.parentId === parentId)
    .map((unit) => (
      <li key={unit.id} style={{ marginLeft: depth * 16 }} className="py-1.5">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-medium">{unit.name}</span>
          <Badge>{UNIT_TYPES[unit.type] ?? unit.type}</Badge>
          {unit.leadUserId && (
            <span className="text-xs text-muted">Responsable : {name(unit.leadUserId)}</span>
          )}
        </div>
        {unit.members.length > 0 && (
          <p className="mt-0.5 text-xs text-muted">{unit.members.map(name).join(' · ')}</p>
        )}
        <ul>
          <UnitTree data={data} parentId={unit.id} depth={depth + 1} />
        </ul>
      </li>
    ));
}
