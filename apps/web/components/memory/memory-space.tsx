import { Trash2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { Badge, Card, EmptyState } from '@/components/ui';
import { formatDate } from '@/lib/format';
import type { MemoryItem } from '@/lib/api';
import { MEMORY_KINDS, type MemorySpaceInfo } from './spaces';

/** Active memories of one space; the user may delete the ones they manage. */
export function MemorySpace({
  space,
  items,
  onDelete,
  children,
}: {
  space: MemorySpaceInfo;
  items: MemoryItem[];
  onDelete: (id: string) => void;
  children?: ReactNode;
}) {
  return (
    <section className="mb-6">
      <h2 className="text-sm font-semibold">{space.title}</h2>
      <p className="mb-2 text-xs text-muted">{space.description}</p>
      {items.length === 0 ? (
        <EmptyState title="Rien pour le moment" />
      ) : (
        <Card className="divide-y divide-line">
          {items.map((m) => (
            <div key={m.id} className="flex items-start gap-3 px-4 py-3">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <p className="text-sm font-medium">{m.title}</p>
                  <Badge>{MEMORY_KINDS[m.kind] ?? m.kind}</Badge>
                  {m.orgUnit && <Badge tone="accent">{m.orgUnit}</Badge>}
                </div>
                <p className="mt-1 text-sm text-muted">{m.content}</p>
                <p className="mt-1 text-xs text-muted">
                  Mis à jour le {formatDate(m.updatedAt)} · utilisé {m.useCount} fois
                </p>
              </div>
              {m.canManage && (
                <button
                  type="button"
                  aria-label={`Supprimer « ${m.title} »`}
                  onClick={() => onDelete(m.id)}
                  className="rounded-md p-1.5 text-muted hover:bg-danger-soft hover:text-danger"
                >
                  <Trash2 className="size-4" />
                </button>
              )}
            </div>
          ))}
        </Card>
      )}
      {children}
    </section>
  );
}
