'use client';

import { Download, FileUp, Trash2 } from 'lucide-react';
import { cx, DISABLED_LOOK } from '@/components/ui';
import type { DocumentItem } from '@/lib/api';
import { REPLACE_PANEL } from './document-replace';

/** Where a document's file is downloaded, as deposited. */
export function documentFileUrl(id: string): string {
  return `/api/documents/${encodeURIComponent(id)}/file`;
}

const ACTION = 'rounded-md p-1.5 text-muted';

/**
 * A document's row actions: its file downloaded, and, for whoever may
 * write its space, a new version chosen in the list's panel (ADR-151),
 * which the button says it opens, or the document deleted. Neither starts
 * while the list or its panel sends (`busy`).
 */
export function DocumentActions({
  document,
  replacing,
  busy,
  register,
  onReplace,
  onRemove,
}: {
  document: DocumentItem;
  /** Its new version is being chosen. */
  replacing: boolean;
  busy: boolean;
  /** Its « Nouvelle version » button, for the focus to come back to. */
  register: (id: string, button: HTMLButtonElement | null) => void;
  onReplace: () => void;
  onRemove: () => void;
}) {
  return (
    <div className="flex justify-end gap-1">
      <a
        href={documentFileUrl(document.id)}
        download
        aria-label={`Télécharger « ${document.title} »`}
        className={cx(ACTION, 'hover:bg-surface-2 hover:text-fg')}
      >
        <Download className="size-4" />
      </a>
      {document.canWrite && (
        <>
          <button
            type="button"
            ref={(button) => register(document.id, button)}
            aria-label={`Nouvelle version de « ${document.title} »`}
            aria-expanded={replacing}
            aria-controls={replacing ? REPLACE_PANEL : undefined}
            disabled={busy}
            onClick={onReplace}
            className={cx(ACTION, 'hover:bg-surface-2 hover:text-fg', DISABLED_LOOK)}
          >
            <FileUp className="size-4" />
          </button>
          <button
            type="button"
            aria-label={`Supprimer « ${document.title} »`}
            disabled={busy}
            onClick={onRemove}
            className={cx(ACTION, 'hover:bg-danger-soft hover:text-danger', DISABLED_LOOK)}
          >
            <Trash2 className="size-4" />
          </button>
        </>
      )}
    </div>
  );
}
