'use client';

import { Download, Trash2 } from 'lucide-react';
import { Badge, Cell, cx, DataTable, DISABLED_LOOK, EmptyState, ErrorNote } from '@/components/ui';
import { api, useAction, type DocumentItem } from '@/lib/api';
import { formatBytes, formatDate } from '@/lib/format';
import { CLEARANCE_LABELS, DOCUMENT_KINDS } from '@/lib/labels';

/** Where a document's file is downloaded, as deposited. */
export function documentFileUrl(id: string): string {
  return `/api/documents/${encodeURIComponent(id)}/file`;
}

/**
 * The documents the reader may read (ADR-095), the latest first: each
 * downloaded as deposited, deleted by whoever may write its space. When
 * older ones are left out, the list says so (ADR-097).
 */
export function DocumentLibrary({
  documents,
  truncated,
  onChanged,
}: {
  documents: DocumentItem[];
  truncated: boolean;
  onChanged: () => void;
}) {
  const action = useAction();

  async function remove(document: DocumentItem) {
    if (!window.confirm(`Supprimer « ${document.title} » ? L’agent ne le lira plus.`)) return;
    if ((await action.run(() => api.delete(`/documents/${document.id}`))) !== null) onChanged();
  }

  if (documents.length === 0) {
    return (
      <EmptyState title="Aucun document">
        Les documents déposés dans vos espaces apparaîtront ici.
      </EmptyState>
    );
  }
  return (
    <section aria-label="Documents">
      {action.error && (
        <div className="mb-3">
          <ErrorNote message={action.error} />
        </div>
      )}
      {truncated && (
        <p className="mb-3 text-xs text-muted">
          Les {documents.length} documents les plus récents sont affichés ; l’agent consulte aussi
          les plus anciens.
        </p>
      )}
      <DataTable columns={['Document', 'Espace', 'Taille', 'Déposé le', 'Actions']}>
        {documents.map((document) => (
          <tr key={document.id}>
            <Cell>
              <p className="font-medium">{document.title}</p>
              <div className="mt-1 flex flex-wrap gap-1">
                <Badge>{DOCUMENT_KINDS[document.kind]}</Badge>
                <Badge>{CLEARANCE_LABELS[document.sensitivity]}</Badge>
              </div>
            </Cell>
            <Cell>
              <Badge tone="accent">{document.orgUnit ?? 'Entreprise'}</Badge>
            </Cell>
            <Cell className="whitespace-nowrap text-muted">
              {formatBytes(document.byteSize)}
              {document.pageCount !== null && ` · ${document.pageCount} p.`}
            </Cell>
            <Cell className="whitespace-nowrap text-muted">{formatDate(document.createdAt)}</Cell>
            <Cell>
              <div className="flex justify-end gap-1">
                <a
                  href={documentFileUrl(document.id)}
                  download
                  aria-label={`Télécharger « ${document.title} »`}
                  className="rounded-md p-1.5 text-muted hover:bg-surface-2 hover:text-fg"
                >
                  <Download className="size-4" />
                </a>
                {document.canWrite && (
                  <button
                    type="button"
                    aria-label={`Supprimer « ${document.title} »`}
                    disabled={action.pending}
                    onClick={() => void remove(document)}
                    className={cx(
                      'rounded-md p-1.5 text-muted hover:bg-danger-soft hover:text-danger',
                      DISABLED_LOOK,
                    )}
                  >
                    <Trash2 className="size-4" />
                  </button>
                )}
              </div>
            </Cell>
          </tr>
        ))}
      </DataTable>
    </section>
  );
}
