'use client';

import { useCallback, useEffect, useRef, useState } from 'react';
import { Badge, Cell, DataTable, EmptyState, ErrorNote } from '@/components/ui';
import { api, useAction, type DocumentItem } from '@/lib/api';
import { formatBytes, formatDate } from '@/lib/format';
import { CLEARANCE_LABELS, DOCUMENT_KINDS } from '@/lib/labels';
import { DocumentActions } from './document-actions';
import { DocumentReplace } from './document-replace';

/**
 * The documents the reader may read (ADR-095), the latest first: each
 * downloaded as deposited, replaced by a new version (ADR-151) or deleted
 * by whoever may write its space, its version said once replaced. When
 * older ones are left out, the list says so (ADR-097).
 *
 * One new version is chosen at a time, in a panel above the list, opened
 * and closed by its row's button: it closes once its document is gone from
 * the reloaded list or may no longer be written. Closed, the focus goes
 * back to its row's button; while it sends, no row's action starts.
 */
export function DocumentLibrary({
  documents,
  truncated,
  maxFileBytes,
  onChanged,
}: {
  documents: DocumentItem[];
  truncated: boolean;
  maxFileBytes: number;
  onChanged: () => void;
}) {
  const action = useAction();
  const [replacingId, setReplacingId] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [replaced, setReplaced] = useState<string | null>(null);
  const replacing = documents.find((d) => d.id === replacingId && d.canWrite) ?? null;
  // Each row's « Nouvelle version » button, to give the focus back to.
  const replaceButtons = useRef(new Map<string, HTMLButtonElement>());
  // The panel closed: whose button takes the focus back, and whether the
  // list is then reloaded, its lock keeping the focus there (ADR-043).
  const closed = useRef<{ id: string; reload: boolean } | null>(null);

  useEffect(() => {
    if (replacingId !== null || closed.current === null) return;
    const { id, reload } = closed.current;
    closed.current = null;
    replaceButtons.current.get(id)?.focus();
    if (reload) onChanged();
  }, [replacingId, onChanged]);

  const register = useCallback((id: string, button: HTMLButtonElement | null) => {
    if (button) replaceButtons.current.set(id, button);
    else replaceButtons.current.delete(id);
  }, []);

  function close(reload: boolean) {
    if (replacingId !== null) closed.current = { id: replacingId, reload };
    setReplacingId(null);
    // Closed, it sends no more: its row's button is enabled for the focus
    // in the same render, before the panel's own cleanup says so.
    setSending(false);
  }

  async function remove(document: DocumentItem) {
    if (!window.confirm(`Supprimer « ${document.title} » ? L’agent ne le lira plus.`)) return;
    if ((await action.run(() => api.delete(`/documents/${document.id}`))) === null) return;
    if (replacingId === document.id) setReplacingId(null);
    onChanged();
  }

  function open(document: DocumentItem) {
    setReplaced(null);
    setReplacingId(document.id);
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
      {replacing && (
        <DocumentReplace
          key={replacing.id}
          document={replacing}
          maxFileBytes={maxFileBytes}
          onCancel={() => close(false)}
          onStale={onChanged}
          onSending={setSending}
          onReplaced={(said) => {
            setReplaced(said);
            close(true);
          }}
        />
      )}
      {replaced && (
        <p role="status" className="mb-3 text-sm text-ok">
          {replaced}
        </p>
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
            <Cell className="whitespace-nowrap text-muted">
              {formatDate(document.createdAt)}
              {document.replacedAt && (
                <p className="mt-1 text-xs">
                  Version {document.version}, remplacée le {formatDate(document.replacedAt)}
                </p>
              )}
            </Cell>
            <Cell>
              <DocumentActions
                document={document}
                replacing={replacing?.id === document.id}
                busy={action.pending || sending}
                register={register}
                onReplace={() => (replacing?.id === document.id ? close(false) : open(document))}
                onRemove={() => void remove(document)}
              />
            </Cell>
          </tr>
        ))}
      </DataTable>
    </section>
  );
}
