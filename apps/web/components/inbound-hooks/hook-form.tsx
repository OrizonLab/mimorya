'use client';

import type { FormEvent } from 'react';
import { Button, ErrorNote, Field, Input, Textarea } from '@/components/ui';
import { useAction } from '@/lib/api';
import { useSyncedState } from '@/lib/use-synced-state';

/** A hook as its form writes it. */
export interface HookDraft {
  title: string;
  instruction: string;
}

/**
 * Writes a hook: its title and the request sent as its author's, which
 * what another system posts follows. Taken again from `initial` when it
 * changes (the list reloaded, ADR-041); `clearOnSave` empties it once
 * saved (a new hook). The API checks what is written; its refusal is shown
 * here.
 */
export function HookForm({
  initial,
  submitLabel,
  clearOnSave = false,
  onSave,
  onCancel,
}: {
  initial: HookDraft;
  submitLabel: string;
  clearOnSave?: boolean;
  onSave: (draft: HookDraft) => Promise<unknown>;
  onCancel?: () => void;
}) {
  const [draft, setDraft] = useSyncedState(initial);
  const action = useAction();
  const set = <K extends keyof HookDraft>(key: K, value: HookDraft[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() => onSave(draft));
    if (saved !== null && clearOnSave) setDraft(initial);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <Field label="Intitulé">
        <Input
          required
          maxLength={120}
          value={draft.title}
          onChange={(e) => set('title', e.target.value)}
          placeholder="Ex. Alerte de la supervision"
        />
      </Field>
      <Field label="Demande à l’agent">
        <Textarea
          required
          rows={3}
          maxLength={4000}
          value={draft.instruction}
          onChange={(e) => set('instruction', e.target.value)}
          placeholder="Ex. Résume l’alerte reçue et propose une intervention si un équipement est en panne."
        />
      </Field>
      <p className="text-xs text-muted">
        Les données envoyées suivent cette demande, comme des informations à vérifier : l’agent ne
        les prend jamais pour des consignes.
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {submitLabel}
        </Button>
        {onCancel && (
          // Not while saving: what is sent may be saved all the same.
          <Button type="button" variant="ghost" disabled={action.pending} onClick={onCancel}>
            Annuler
          </Button>
        )}
      </div>
    </form>
  );
}
