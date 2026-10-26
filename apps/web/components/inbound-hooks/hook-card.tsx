'use client';

import { useEffect, useRef, useState } from 'react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import {
  api,
  useAction,
  type InboundHook,
  type InboundHookToken,
  type InboundHookUpdate,
} from '@/lib/api';
import { HookForm, type HookDraft } from './hook-form';
import { LastDelivery } from './last-delivery';

/**
 * One of the reader's inbound hooks: the address another system posts to,
 * what it asks, what became of its last delivery; changed, paused or
 * resumed, given a new token and deleted from here. Resuming one or
 * renewing its token needs the right to use the agent; pausing or deleting
 * it does not (ADR-109). A new token is handed to `onToken`, to be shown
 * once.
 */
export function HookCard({
  hook,
  onChanged,
  onToken,
}: {
  hook: InboundHook;
  onChanged: () => void;
  onToken: (title: string, issued: InboundHookToken) => void;
}) {
  const [editing, setEditing] = useState(false);
  const action = useAction();
  const url = `/inbound-hooks/${hook.id}`;
  const editButton = useRef<HTMLButtonElement>(null);
  // How the form closed, until « Modifier » has the focus back.
  const closed = useRef<'cancelled' | 'saved' | null>(null);

  // The form gone, « Modifier » takes the focus back; then the list reloads
  // if it saved, whose lock keeps the focus there (ADR-043).
  useEffect(() => {
    if (editing || closed.current === null) return;
    const saved = closed.current === 'saved';
    closed.current = null;
    editButton.current?.focus();
    if (saved) onChanged();
  }, [editing, onChanged]);

  function close(how: 'cancelled' | 'saved') {
    closed.current = how;
    setEditing(false);
  }

  async function change(body: InboundHookUpdate) {
    if ((await action.run(() => api.patch(url, body))) !== null) onChanged();
  }

  async function renew() {
    const confirmed = window.confirm(
      `Remplacer le jeton de « ${hook.title} » ? L’ancien cesse aussitôt de servir : le système qui l’utilise doit recevoir le nouveau.`,
    );
    if (!confirmed) return;
    const issued = await action.run(() => api.post<InboundHookToken>(`${url}/token`, {}));
    if (issued !== null) onToken(hook.title, issued);
  }

  async function remove() {
    if (!window.confirm(`Supprimer le déclencheur « ${hook.title} » ?`)) return;
    if ((await action.run(() => api.delete(url))) !== null) onChanged();
  }

  async function save(draft: HookDraft) {
    await api.patch(url, {
      title: draft.title,
      instruction: draft.instruction,
    } satisfies InboundHookUpdate);
    close('saved');
  }

  return (
    // Named by its title: each hook is found by it.
    <article aria-label={hook.title}>
      <Card className="flex flex-col gap-2 p-4">
        <div className="flex flex-wrap items-center gap-2">
          <p className="font-medium">{hook.title}</p>
          <Badge tone={hook.enabled ? 'ok' : 'neutral'}>
            {hook.enabled ? 'Actif' : 'En pause'}
          </Badge>
        </div>
        <p className="text-xs text-muted">
          Adresse :{' '}
          <code className="rounded bg-surface-2 px-1 break-all select-all">{hook.url}</code>
        </p>
        <p className="text-sm whitespace-pre-line text-muted">{hook.instruction}</p>
        <LastDelivery hook={hook} />
        {editing ? (
          <HookForm
            initial={{ title: hook.title, instruction: hook.instruction }}
            submitLabel="Enregistrer"
            onSave={save}
            onCancel={() => close('cancelled')}
          />
        ) : (
          <div className="flex flex-wrap gap-2">
            <Button
              ref={editButton}
              variant="secondary"
              aria-label={`Modifier « ${hook.title} »`}
              disabled={action.pending}
              onClick={() => setEditing(true)}
            >
              Modifier
            </Button>
            <Button
              variant="secondary"
              aria-label={`${hook.enabled ? 'Mettre en pause' : 'Reprendre'} « ${hook.title} »`}
              loading={action.pending}
              onClick={() => void change({ enabled: !hook.enabled } satisfies InboundHookUpdate)}
            >
              {hook.enabled ? 'Mettre en pause' : 'Reprendre'}
            </Button>
            <Button
              variant="secondary"
              aria-label={`Nouveau jeton pour « ${hook.title} »`}
              disabled={action.pending}
              onClick={() => void renew()}
            >
              Nouveau jeton
            </Button>
            <Button
              variant="danger"
              aria-label={`Supprimer « ${hook.title} »`}
              disabled={action.pending}
              onClick={() => void remove()}
            >
              Supprimer
            </Button>
          </div>
        )}
        {action.error && <ErrorNote message={action.error} />}
      </Card>
    </article>
  );
}
