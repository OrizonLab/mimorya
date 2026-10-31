'use client';

import { useState } from 'react';
import { Button, Card, ErrorNote } from '@/components/ui';
import {
  api,
  useAction,
  type DocumentItem,
  type DocumentReplaced,
  type DocumentReplacement as Replacement,
} from '@/lib/api';
import { FileDrop, refuseBeyond } from './file-drop';

/** The replacement's query string: the new file's name, the version replaced. */
function query(replacement: Replacement): string {
  return new URLSearchParams({
    name: replacement.name,
    version: String(replacement.version),
  }).toString();
}

/**
 * A new version of `document` (ADR-150, ADR-151): its file, chosen or
 * dropped, replaces the version the reader sees. The document keeps its
 * space and sensitivity; the agent reads only the new version, the former
 * file not kept. Replaced in between by someone else, the API refuses it
 * and says so. A file beyond the largest the API takes is not even sent.
 */
export function DocumentReplace({
  document,
  maxFileBytes,
  onReplaced,
  onCancel,
}: {
  document: DocumentItem;
  maxFileBytes: number;
  /** Told what became of it, once replaced. */
  onReplaced: (said: string) => void;
  onCancel: () => void;
}) {
  const action = useAction();
  const [file, setFile] = useState<File | null>(null);

  async function replace() {
    if (!file) return;
    const replacement = { name: file.name, version: document.version } satisfies Replacement;
    const path = `/documents/${encodeURIComponent(document.id)}/file?${query(replacement)}`;
    const replaced = await action.run(() => {
      refuseBeyond(file, maxFileBytes);
      return api.replace<DocumentReplaced>(path, file);
    });
    if (replaced === null) return;
    onReplaced(
      `« ${document.title} » est remplacé par « ${file.name} » : version ${replaced.version}, ${replaced.passages} passage(s) pour l’agent.`,
    );
  }

  return (
    <section aria-labelledby="new-version" className="mb-4">
      <Card className="flex flex-col gap-3 p-4">
        <h2 id="new-version" className="font-medium">
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
