'use client';

import { useRef, useState, type DragEvent } from 'react';
import { cx, Field, Input } from '@/components/ui';
import { ACCEPTED } from './file-rules';

/** Whether what is dragged carries files. */
function carriesFiles(event: DragEvent): boolean {
  return Array.from(event.dataTransfer.types).includes('Files');
}

/**
 * A file chosen or dropped (ADR-151): the form's file input, labelled
 * « Fichier », inside a zone a file can be dropped on. A dropped file is
 * taken as a chosen one, the first only, which the input shows too where
 * the browser lets it (it is emptied otherwise); the zone says which file
 * is taken. While the form is busy, nothing dropped is taken, nor opened
 * by the browser instead. `reset` changes once the file is sent: a new
 * input, empty.
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
    if (!carriesFiles(event)) return;
    // The file is dropped here rather than opened by the browser, even
    // while busy: then nothing is taken.
    event.preventDefault();
    event.dataTransfer.dropEffect = disabled ? 'none' : 'copy';
    setOver(!disabled);
  }

  function drop(event: DragEvent) {
    if (!carriesFiles(event)) return;
    event.preventDefault();
    setOver(false);
    const dropped = event.dataTransfer.files[0];
    if (disabled || !dropped) return;
    shown(dropped);
    onFile(dropped);
  }

  /** The input shows `dropped` alone, or nothing where it cannot. */
  function shown(dropped: File) {
    const field = input.current;
    if (!field) return;
    try {
      const only = new DataTransfer();
      only.items.add(dropped);
      field.files = only.files;
    } catch {
      field.value = '';
    }
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
