'use client';

import { useRef, useState, type DragEvent } from 'react';
import { cx, Field, Input } from '@/components/ui';
import { ApiError } from '@/lib/api';
import { formatBytes } from '@/lib/format';

/** The files the API reads (ADR-095). */
export const ACCEPTED = '.pdf,.docx,.md,.markdown,.txt';

/**
 * Refuses, before it is sent, a file beyond the largest the API takes,
 * which it gives with the list (ADR-097).
 */
export function refuseBeyond(file: File, maxFileBytes: number): void {
  if (file.size > maxFileBytes) {
    throw new ApiError(
      413,
      'too_large',
      `Ce fichier dépasse ${formatBytes(maxFileBytes)} : il n’est pas envoyé.`,
    );
  }
}

/** Whether what is dragged carries files. */
function carriesFiles(event: DragEvent): boolean {
  return Array.from(event.dataTransfer.types).includes('Files');
}

/**
 * A file chosen or dropped (ADR-151): the form's file input, labelled
 * « Fichier », inside a zone a file can be dropped on. A dropped file is
 * taken as a chosen one, the first only; the zone says which file is
 * taken. While the form is busy, nothing dropped is taken. `reset` changes
 * once the file is sent: a new input, empty.
 */
export function FileDrop({
  file,
  onFile,
  disabled,
  reset,
}: {
  file: File | null;
  onFile: (file: File | null) => void;
  disabled?: boolean;
  reset?: number;
}) {
  const input = useRef<HTMLInputElement>(null);
  const [over, setOver] = useState(false);

  function dragOver(event: DragEvent) {
    if (disabled || !carriesFiles(event)) return;
    // Lets the file be dropped here rather than opened by the browser.
    event.preventDefault();
    event.dataTransfer.dropEffect = 'copy';
    setOver(true);
  }

  function drop(event: DragEvent) {
    if (!carriesFiles(event)) return;
    event.preventDefault();
    setOver(false);
    const dropped = event.dataTransfer.files[0];
    if (disabled || !dropped) return;
    // The input shows it too, where the browser lets it.
    if (input.current && event.dataTransfer.files.length === 1) {
      input.current.files = event.dataTransfer.files;
    }
    onFile(dropped);
  }

  return (
    <div
      onDragOver={dragOver}
      onDragLeave={() => setOver(false)}
      onDrop={drop}
      className={cx(
        'rounded-lg border border-dashed border-line p-3',
        over && 'border-accent bg-accent-soft',
      )}
    >
      <Field label="Fichier">
        <Input
          key={reset}
          ref={input}
          type="file"
          accept={ACCEPTED}
          disabled={disabled}
          onChange={(e) => onFile(e.target.files?.[0] ?? null)}
        />
      </Field>
      <p className="mt-2 text-xs text-muted">
        {file ? `Fichier retenu : ${file.name}` : 'Choisissez un fichier, ou déposez-le ici.'}
      </p>
    </div>
  );
}
