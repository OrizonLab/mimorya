'use client';

import { useState, type FormEvent } from 'react';
import { Button, cx, DISABLED_LOOK } from '@/components/ui';

const FIELD = cx(
  'rounded-lg border border-line bg-surface px-3 py-2 text-sm outline-none focus:border-accent',
  DISABLED_LOOK,
);

/** Adds a personal preference; the fields are cleared once it is saved. */
export function PersonalMemoryForm({
  onAdd,
}: {
  onAdd: (title: string, content: string) => Promise<boolean>;
}) {
  const [title, setTitle] = useState('');
  const [content, setContent] = useState('');
  const [saving, setSaving] = useState(false);

  async function submit(event: FormEvent) {
    event.preventDefault();
    setSaving(true);
    if (await onAdd(title, content)) {
      setTitle('');
      setContent('');
    }
    setSaving(false);
  }

  return (
    <form onSubmit={submit} className="mt-3 flex flex-col gap-2 sm:flex-row">
      <input
        required
        minLength={3}
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        placeholder="Intitulé (ex. Format préféré)"
        aria-label="Intitulé"
        className={cx(FIELD, 'sm:w-56')}
      />
      <input
        required
        minLength={3}
        value={content}
        onChange={(e) => setContent(e.target.value)}
        placeholder="Ce que l’agent doit retenir"
        aria-label="Contenu"
        className={cx(FIELD, 'flex-1')}
      />
      <Button type="submit" variant="secondary" loading={saving}>
        Ajouter
      </Button>
    </form>
  );
}
