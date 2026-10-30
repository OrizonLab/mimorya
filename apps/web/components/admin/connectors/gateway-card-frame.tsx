'use client';

import { useState, type ReactNode } from 'react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import type { GatewayView } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { useGatewayActions } from './use-gateway-actions';

/**
 * What every conversation gateway's card shows and does (ADR-126, ADR-134,
 * ADR-139): its name, its service and whether it is active, what it says
 * of itself (`children`), when it changed; changed with its own form,
 * checked with its service when it can be (`check`, its button's label),
 * switched on or off, and deleted once `removal` is confirmed.
 */
export function GatewayCardFrame({
  gateway,
  product,
  check,
  removal,
  form,
  onChanged,
  children,
}: {
  gateway: GatewayView;
  /** The service, on its badge. */
  product: string;
  check?: string;
  removal: string;
  /** Its own form, which calls `done` once closed. */
  form: (done: (changed: boolean) => void) => ReactNode;
  onChanged: () => void;
  children: ReactNode;
}) {
  const [editing, setEditing] = useState(false);
  const actions = useGatewayActions(gateway, onChanged);
  const { action, loading, held } = actions;

  function done(changed: boolean) {
    setEditing(false);
    if (changed) onChanged();
  }

  return (
    <Card className="flex flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 font-medium">{gateway.name}</p>
        <Badge>{product}</Badge>
        <Badge tone={gateway.enabled ? 'ok' : 'neutral'}>
          {gateway.enabled ? 'Active' : 'Désactivée'}
        </Badge>
      </div>
      {children}
      <p className="text-xs text-muted">Modifiée le {formatDate(gateway.updatedAt)}</p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex flex-wrap gap-2">
        {!editing && (
          <Button variant="secondary" onClick={() => setEditing(true)}>
            Modifier
          </Button>
        )}
        {check && (
          <Button
            variant="secondary"
            disabled={held('check')}
            loading={loading('check')}
            onClick={actions.check}
          >
            {check}
          </Button>
        )}
        <Button
          variant="secondary"
          disabled={held('toggle')}
          loading={loading('toggle')}
          onClick={actions.toggle}
        >
          {gateway.enabled ? 'Désactiver' : 'Activer'}
        </Button>
        <Button
          variant="danger"
          disabled={held('remove')}
          loading={loading('remove')}
          onClick={() => actions.remove(removal)}
        >
          Supprimer
        </Button>
      </div>
      {editing && form(done)}
    </Card>
  );
}
