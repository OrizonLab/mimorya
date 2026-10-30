'use client';

import { Trash2 } from 'lucide-react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type KnowledgeShareView, type ShareableUnit } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { CLEARANCE_LABELS } from '@/lib/labels';

/** One share, in plain French: whose knowledge, read by whom, what and how far; removable. */
export function ShareCard({
  share,
  units,
  onChanged,
}: {
  share: KnowledgeShareView;
  units: ShareableUnit[];
  onChanged: () => void;
}) {
  const action = useAction();
  const names = new Map(units.map((u) => [u.id, u.name]));
  const source = names.get(share.sourceOrgUnitId) ?? 'Unité inconnue';
  const target = names.get(share.targetOrgUnitId) ?? 'Unité inconnue';
  const title = `${source} → ${target}`;

  async function remove() {
    if (!window.confirm(`Retirer le partage « ${title} » ?`)) return;
    if (await action.run(() => api.delete(`/admin/knowledge-shares/${share.id}`))) onChanged();
  }

  return (
    <Card className="p-4">
      <div className="flex flex-wrap items-center gap-2">
        <p className="font-medium">{title}</p>
        {share.memory && <Badge>Mémoire</Badge>}
        {share.documents && <Badge>Documents</Badge>}
        {ended(share.expiresAt) && <Badge tone="warn">Échu</Badge>}
        <Button
          variant="ghost"
          className="ml-auto px-2 py-1"
          onClick={remove}
          loading={action.pending}
        >
          <Trash2 className="size-3.5" aria-hidden />
          <span className="sr-only">Retirer {title}</span>
        </Button>
      </div>
      <p className="mt-2 text-sm text-muted">
        {readers(target, share.includeDescendants)}, jusqu’au niveau «{' '}
        {CLEARANCE_LABELS[share.maxSensitivity]} » ;{' '}
        {share.expiresAt ? `jusqu’au ${formatDate(share.expiresAt)}` : 'sans échéance'}.
      </p>
      {action.error && <ErrorNote message={action.error} />}
    </Card>
  );
}

/** Who reads what a share opens. */
function readers(target: string, below: boolean): string {
  const unit = `Lu par les membres de l’unité « ${target} »`;
  return below ? `${unit} et des unités en dessous` : unit;
}

/** Whether a share past this date opens nothing any more. */
function ended(expiresAt: string | null): boolean {
  return expiresAt !== null && new Date(expiresAt).getTime() <= Date.now();
}
