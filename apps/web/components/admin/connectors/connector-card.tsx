'use client';

import { useState } from 'react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import { api, useAction, type ConnectorTest, type ConnectorView, type Connectors } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { McpForm } from './mcp-form';
import { ToolPolicies } from './tool-policies';

const STATUS = {
  untested: { label: 'Non testé', tone: 'neutral' },
  ok: { label: 'Opérationnel', tone: 'ok' },
  error: { label: 'En erreur', tone: 'danger' },
  disabled: { label: 'Désactivé', tone: 'neutral' },
} as const;

/** One connector: its state, a connection test, and what may change here. */
export function ConnectorCard({
  connector,
  data,
  onChanged,
}: {
  connector: ConnectorView;
  data: Connectors;
  onChanged: () => void;
}) {
  const [editing, setEditing] = useState(false);
  const [tested, setTested] = useState<ConnectorTest | null>(null);
  const action = useAction();
  const type = data.types.find((t) => t.type === connector.type);
  const status = STATUS[connector.status] ?? STATUS.error;

  async function test() {
    setTested(
      await action.run(() => api.post<ConnectorTest>(`/admin/connectors/${connector.id}/test`)),
    );
    onChanged();
  }

  async function toggle() {
    const enabled = !connector.enabled;
    if (await action.run(() => api.patch(`/admin/connectors/${connector.id}`, { enabled }))) {
      onChanged();
    }
  }

  async function remove() {
    if (!window.confirm(`Supprimer le connecteur « ${connector.name} » et son jeton ?`)) return;
    if (await action.run(() => api.delete(`/admin/connectors/${connector.id}`))) onChanged();
  }

  return (
    <Card className="flex flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center gap-2">
        <p className="flex-1 font-medium">{connector.name}</p>
        <Badge>{type?.name ?? connector.type}</Badge>
        {type?.demo && <Badge tone="warn">Démonstration</Badge>}
        <Badge tone={status.tone}>{status.label}</Badge>
      </div>
      {connector.mcp && (
        <p className="text-sm text-muted">
          <code>{connector.mcp.url}</code> · outils <code>mcp_{connector.mcp.prefix}_…</code> ·{' '}
          {connector.mcp.tokenStored ? 'jeton enregistré' : 'sans jeton'}
        </p>
      )}
      {connector.lastError && connector.status === 'error' && !tested && (
        <ErrorNote message={`Dernier démarrage : ${connector.lastError}`} />
      )}
      <p className="text-xs text-muted">Modifié le {formatDate(connector.updatedAt)}</p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" loading={action.pending} onClick={test}>
          Tester la connexion
        </Button>
        {connector.mcp && !editing && (
          <Button variant="secondary" onClick={() => setEditing(true)}>
            Modifier
          </Button>
        )}
        <Button variant="secondary" onClick={toggle}>
          {connector.enabled ? 'Désactiver' : 'Activer'}
        </Button>
        {connector.mcp && (
          <Button variant="danger" onClick={remove}>
            Supprimer
          </Button>
        )}
      </div>
      {editing && (
        <McpForm
          data={data}
          connector={connector}
          onDone={(changed) => {
            setEditing(false);
            if (changed) onChanged();
          }}
        />
      )}
      {tested && !tested.ok && (
        <ErrorNote message={`Échec : ${tested.error ?? 'erreur inconnue'}`} />
      )}
      {tested?.ok && (
        <section className="flex flex-col gap-2">
          <p className="text-sm text-ok">
            Connexion réussie : {tested.tools.length} outil(s) exposé(s).
          </p>
          {connector.mcp && tested.tools.length > 0 && (
            <ToolPolicies
              connectorId={connector.id}
              data={data}
              tools={tested.tools}
              onSaved={() => {
                setTested(null);
                onChanged();
              }}
            />
          )}
        </section>
      )}
    </Card>
  );
}
