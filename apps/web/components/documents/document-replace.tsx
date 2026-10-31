'use client';

import { useEffect, useRef, useState } from 'react';
import { Button, Card, ErrorNote } from '@/components/ui';
import {
  api,
  ApiError,
  useAction,
  type DocumentItem,
  type DocumentReplaced,
  type DocumentReplacement as Replacement,
} from '@/lib/api';
import { FileDrop } from './file-drop';
import { refuseBeyond } from './file-rules';

/** The panel's element, which its row's button controls. */
export const REPLACE_PANEL = 'new-version-panel';

/** The replacement's query string: the new file's name, the version replaced. */
function query(replacement: Replacement): string {
  return new URLSearchParams({
    name: replacement.name,
    version: String(replacement.version),
  }).toString();
}

/**
 * A new version of `document` (ADR-150, ADR-151): its file, chosen or
 * dropped, replaces the version the panel says, the list's. The document
 * keeps its space and sensitivity; the agent reads only the new version,
 * the former file not kept. Refused as replaced in between, or for the
 * company's quota (both a conflict), the list is reloaded and the panel
 * stays, its reason shown, saying the version now there. A file beyond the
 * largest the API takes is not even sent. Its title takes the focus when it
 * opens; it tells the list while it sends.
 */
export function DocumentReplace({
  document,
  maxFileBytes,
  onReplaced,
  onCancel,
  onConflict,
  onSending,
}: {
  document: DocumentItem;
  maxFileBytes: number;
  /** Told what became of it, once replaced. */
  onReplaced: (said: string) => void;
  onCancel: () => void;
  /** Refused for a conflict: the list is to be read again. */
  onConflict: () => void;
  onSending: (sending: boolean) => void;
}) {
  const action = useAction();
  const [file, setFile] = useState<File | null>(null);
  const heading = useRef<HTMLHeadingElement>(null);

  useEffect(() => heading.current?.focus(), []);
  useEffect(() => {
    onSending(action.pending);
    return () => onSending(false);
  }, [action.pending, onSending]);

  async function replace() {
    if (!file) return;
    const replacement = { name: file.name, version: document.version } satisfies Replacement;
    const path = `/documents/${encodeURIComponent(document.id)}/file?${query(replacement)}`;
    let conflict = false;
    const replaced = await action.run(async () => {
      refuseBeyond(file, maxFileBytes);
      try {
        return await api.replace<DocumentReplaced>(path, file);
      } catch (error) {
        conflict = error instanceof ApiError && error.status === 409;
        throw error;
      }
    });
    if (replaced === null) {
      if (conflict) onConflict();
      return;
    }
    onReplaced(
      `« ${document.title} » est remplacé par « ${file.name} » : version ${replaced.version}, ${replaced.passages} passage(s) pour l’agent.`,
    );
  }

  return (
    <section id={REPLACE_PANEL} aria-labelledby="new-version" className="mb-4">
      <Card className="flex flex-col gap-3 p-4">
        <h2 id="new-version" ref={heading} tabIndex={-1} className="font-medium outline-none">
          Nouvelle version de « {document.title} »
        </h2>
        <p className="text-xs text-muted">
          Elle remplace la version {document.version} : l’agent ne lira plus que la nouvelle, et
          l’ancien fichier n’est pas gardé. Le document garde son espace et sa sensibilité.
        </p>
        <FileDrop file={file} onFile={setFile} disabled={action.pending} />
        <div className="flex gap-2">
          <Button onClick={replace} loading={action.pending} disabled={!file}>
            Remplacer
          </Button>
          <Button variant="secondary" onClick={onCancel} disabled={action.pending}>
            Annuler
          </Button>
        </div>
        {action.error && <ErrorNote message={action.error} />}
      </Card>
    </section>
  );
}
