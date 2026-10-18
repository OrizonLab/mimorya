'use client';

import { Pencil, Trash2 } from 'lucide-react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction } from '@/lib/api';
import { describeCondition } from './describe';
import { PolicyForm } from './policy-form';
import type { PoliciesView, PolicyView } from './types';

/** One policy, in plain French; deny policies can be switched, edited, deleted. */
export function PolicyCard({
  policy,
  data,
  editing,
  onEdit,
  onChanged,
}: {
  policy: PolicyView;
  data: PoliciesView;
  editing: boolean;
  onEdit: (editing: boolean) => void;
  onChanged: () => void;
}) {
  const action = useAction();
  const permissionLabels = new Map(data.permissions.map((p) => [p.permission, p.label]));
  const attributeLabels = new Map(data.attributes.map((a) => [a.path, a.label]));

  async function toggle() {
    const body = { enabled: !policy.enabled };
    if (await action.run(() => api.patch(`/admin/policies/${policy.id}`, body))) onChanged();
  }

  async function remove() {
    if (!window.confirm(`Supprimer la politique « ${policy.description} » ?`)) return;
    if (await action.run(() => api.delete(`/admin/policies/${policy.id}`))) onChanged();
  }

  if (editing) {
    return (
      <Card className="p-4">
        <p className="mb-3 font-medium">Modifier la politique</p>
        <PolicyForm
          data={data}
          policy={policy}
          onDone={(changed) => {
            onEdit(false);
            if (changed) onChanged();
          }}
        />
      </Card>
    );
  }

  return (
    <Card className="p-4">
      <div className="flex flex-wrap items-center gap-2">
        <p className="font-medium">{policy.description}</p>
        <Badge tone={policy.effect === 'deny' ? 'warn' : 'neutral'}>
          {policy.effect === 'deny' ? 'Refus' : 'Autorisation'}
        </Badge>
        {!policy.enabled && <Badge tone="neutral">Désactivée</Badge>}
        {policy.editable ? (
          <span className="ml-auto flex gap-1">
            <Button variant="ghost" className="px-2 py-1" onClick={toggle} loading={action.pending}>
              {policy.enabled ? 'Désactiver' : 'Activer'}
            </Button>
            <Button variant="ghost" className="px-2 py-1" onClick={() => onEdit(true)}>
              <Pencil className="size-3.5" aria-hidden />
              <span className="sr-only">Modifier {policy.description}</span>
            </Button>
            <Button variant="ghost" className="px-2 py-1" onClick={remove}>
              <Trash2 className="size-3.5" aria-hidden />
              <span className="sr-only">Supprimer {policy.description}</span>
            </Button>
          </span>
        ) : (
          <span className="ml-auto text-xs text-muted">Lecture seule</span>
        )}
      </div>
      <p className="mt-2 text-sm">
        <span className="text-muted">Quand : </span>
        {describeCondition(policy.condition, attributeLabels)}
      </p>
      <ul className="mt-2 flex flex-wrap gap-1">
        {policy.permissions.map((p) => (
          <li key={p}>
            <Badge>{permissionLabels.get(p) ?? p}</Badge>
          </li>
        ))}
      </ul>
      {action.error && <ErrorNote message={action.error} />}
    </Card>
  );
}
