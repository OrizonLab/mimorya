'use client';

import { useState } from 'react';
import { HookCard, HookForm, TokenNotice, type HookDraft } from '@/components/inbound-hooks';
import { Card, EmptyState, Loaded, PageHeader } from '@/components/ui';
import {
  api,
  useApiData,
  type InboundHookList,
  type InboundHookToken,
  type NewInboundHook,
} from '@/lib/api';
import { useCan } from '@/lib/me-context';

const BLANK: HookDraft = { title: '', instruction: '' };

/** A token just made or renewed, and the hook it is for. */
interface Issued {
  title: string;
  issued: InboundHookToken;
}

export default function InboundHooksPage() {
  const state = useApiData<InboundHookList>('/inbound-hooks');
  const mayUseAgent = useCan('agent:use');
  // Shown once: never stored, gone with the page.
  const [issued, setIssued] = useState<Issued | null>(null);

  async function create(draft: HookDraft) {
    const made = await api.post<InboundHookToken>('/inbound-hooks', {
      title: draft.title,
      instruction: draft.instruction,
    } satisfies NewInboundHook);
    setIssued({ title: draft.title.trim(), issued: made });
    state.reload();
  }

  return (
    <div className="mx-auto w-full max-w-4xl px-4 py-8">
      <PageHeader
        title="Déclencheurs"
        description="Une adresse que vos autres outils (supervision, formulaire, gestion des tickets) appellent pour confier une demande à l’agent : chaque envoi part dans une nouvelle conversation, avec les données reçues et vos droits du moment. Ce que l’agent ferait passe toujours par votre validation, dans « Validations »."
      />
      {issued && (
        <TokenNotice
          // Another token is another notice: nothing said of the former stays.
          key={issued.issued.token}
          title={issued.title}
          issued={issued.issued}
          onClose={() => setIssued(null)}
        />
      )}
      <Loaded state={state}>
        {({ hooks, maxHooks, maxPerHour, maxBodyBytes }) => (
          <>
            <section aria-labelledby="new-hook" className="mb-6">
              <Card className="flex flex-col gap-3 p-4">
                <h2 id="new-hook" className="font-medium">
                  Nouveau déclencheur
                </h2>
                {!mayUseAgent ? (
                  <p className="text-sm text-muted">
                    Vous n’avez pas le droit d’utiliser l’agent : vous ne pouvez pas créer de
                    déclencheur.
                  </p>
                ) : hooks.length >= maxHooks ? (
                  <p className="text-sm text-muted">
                    Vous avez {maxHooks} déclencheurs, le maximum : supprimez-en un d’abord.
                  </p>
                ) : (
                  <HookForm initial={BLANK} submitLabel="Créer" clearOnSave onSave={create} />
                )}
                <p className="text-xs text-muted">
                  Chaque déclencheur reçoit au plus {maxPerHour} envois par heure, de{' '}
                  {Math.round(maxBodyBytes / 1024)} Kio de texte au plus chacun.
                </p>
              </Card>
            </section>
            {hooks.length === 0 ? (
              <EmptyState title="Aucun déclencheur" />
            ) : (
              <div className="flex flex-col gap-3">
                {hooks.map((hook) => (
                  <HookCard
                    key={hook.id}
                    hook={hook}
                    onChanged={state.reload}
                    onToken={(title, token) => setIssued({ title, issued: token })}
                  />
                ))}
              </div>
            )}
          </>
        )}
      </Loaded>
    </div>
  );
}
