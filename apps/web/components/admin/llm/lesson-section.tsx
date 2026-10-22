'use client';

import type { FormEvent } from 'react';
import { Button, Card, Checkbox, ErrorNote, Loaded } from '@/components/ui';
import {
  api,
  useAction,
  useApiData,
  type MemorySettingsUpdate,
  type MemorySettingsView,
} from '@/lib/api';
import { useSyncedState } from '@/lib/use-synced-state';

/** Whether finished conversations are reread for a lesson (ADR-076). */
export function LessonSection() {
  const state = useApiData<MemorySettingsView>('/admin/memory-settings');
  return (
    <Loaded state={state}>{(data) => <LessonForm data={data} onSaved={state.reload} />}</Loaded>
  );
}

function LessonForm({ data, onSaved }: { data: MemorySettingsView; onSaved: () => void }) {
  // Started again from each reloaded setting (ADR-041).
  const [capture, setCapture] = useSyncedState(data.captureLessons);
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    const body = { captureLessons: capture } satisfies MemorySettingsUpdate;
    if (await action.run(() => api.put('/admin/memory-settings', body))) onSaved();
  }

  return (
    <Card className="p-4">
      <form
        aria-label="Leçons tirées des conversations"
        onSubmit={submit}
        className="flex flex-col gap-3"
      >
        <p className="font-medium">Leçons tirées des conversations</p>
        <label className="flex items-center gap-2 text-sm">
          <Checkbox checked={capture} onChange={(e) => setCapture(e.target.checked)} />
          Proposer à chacun les leçons de ses conversations
        </label>
        <p className="text-xs text-muted">
          Après chaque réponse de l’assistant qui a utilisé des outils, le modèle de vos
          conversations relit la demande et propose au plus une leçon (une façon de faire qui a
          fonctionné, un piège évité, une préférence exprimée) dans la mémoire personnelle de la
          personne pour qui il a agi. Elle seule la voit et la valide dans « Mémoire » ; une leçon
          refusée n’est pas reproposée sous le même titre. Chaque relecture est comptée dans la
          consommation.
        </p>
        {action.error && <ErrorNote message={action.error} />}
        <Button
          type="submit"
          className="w-fit"
          loading={action.pending}
          disabled={capture === data.captureLessons}
        >
          Enregistrer
        </Button>
      </form>
    </Card>
  );
}
