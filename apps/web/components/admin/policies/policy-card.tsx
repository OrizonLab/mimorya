'use client';

import { Pencil, Trash2 } from 'lucide-react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type Policies, type PolicyUpdate, type PolicyView } from '@/lib/api';
import { describeCondition } from './describe';
import { GrantForm } from './grant-form';
import { PolicyForm } from './policy-form';

/**
 * One policy, in plain French. A deny policy is switched, edited and deleted
 * at once; an allow policy is switched off or deleted at once, which only
 * takes access away, and changed through a proposal (ADR-168).
 */
export function PolicyCard({
  policy,
  data,
  editing,
  onEdit,
  onChanged,
}: {
  policy: PolicyView;
  data: Policies;
  editing: boolean;
  onEdit: (editing: boolean) => void;
  onChanged: () => void;
}) {
  const action = useAction();
  const permissionLabels = new Map(data.permissions.map((p) => [p.permission, p.label]));
  const attributeLabels = new Map(data.attributes.map((a) => [a.path, a.label]));

  async function toggle() {
    const body = { enabled: !policy.enabled } satisfies PolicyUpdate;
    if (await action.run(() => api.patch(`/admin/policies/${policy.id}`, body))) onChanged();
  }

  async function remove() {
    const question = policy.editable
      ? `Supprimer la politique « ${policy.description} » ?`
      : `Supprimer l’autorisation « ${policy.description} » ? Les propositions qui la modifient sont supprimées avec elle.`;
    if (!window.confirm(question)) return;
    if (await action.run(() => api.delete(`/admin/policies/${policy.id}`))) onChanged();
  }

  if (editing) {
    const done = (changed: boolean) => {
      onEdit(false);
      if (changed) onChanged();
    };
    return (
      <Card className="p-4">
        <p className="mb-3 font-medium">
          {policy.editable ? 'Modifier la politique' : 'Proposer une modification'}
        </p>
        {policy.editable ? (
          <PolicyForm data={data} policy={policy} onDone={done} />
        ) : (
          <GrantForm data={data} policy={policy} onDone={done} />
        )}
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
        <span className="ml-auto flex gap-1">
          {/* An allow policy is switched on again only through a proposal. */}
          {(policy.editable || policy.enabled) && (
            <Button variant="ghost" className="px-2 py-1" onClick={toggle} loading={action.pending}>
              {policy.enabled ? 'Désactiver' : 'Activer'}
            </Button>
          )}
          <Button variant="ghost" className="px-2 py-1" onClick={() => onEdit(true)}>
            <Pencil className="size-3.5" aria-hidden />
            <span className="sr-only">
              {policy.editable ? 'Modifier' : 'Proposer une modification de'} {policy.description}
            </span>
          </Button>
          <Button variant="ghost" className="px-2 py-1" onClick={remove}>
            <Trash2 className="size-3.5" aria-hidden />
            <span className="sr-only">Supprimer {policy.description}</span>
          </Button>
        </span>
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
      {!policy.editable && !policy.enabled && (
        <p className="mt-2 text-xs text-muted">
          Une autorisation se réactive par une proposition qu’un autre administrateur approuve.
        </p>
      )}
      {action.error && <ErrorNote message={action.error} />}
    </Card>
  );
}
