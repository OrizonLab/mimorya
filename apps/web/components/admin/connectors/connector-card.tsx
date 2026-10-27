'use client';

import { useState } from 'react';
import { Badge, Button, Card, ErrorNote } from '@/components/ui';
import {
  api,
  useAction,
  type ConnectorTest,
  type ConnectorUpdate,
  type ConnectorView,
  type Connectors,
} from '@/lib/api';
import { formatDate } from '@/lib/format';
import { McpForm } from './mcp-form';
import { RestForm } from './rest-form';
import { ToolPolicies } from './tool-policies';

const STATUS = {
  untested: { label: 'Non testé', tone: 'neutral' },
  ok: { label: 'Opérationnel', tone: 'ok' },
  error: { label: 'En erreur', tone: 'danger' },
  disabled: { label: 'Désactivé', tone: 'neutral' },
} as const;

/** A REST API's test checks its declaration, without calling it. */
const REST_STATUS: Partial<Record<keyof typeof STATUS, { label: string; tone: 'neutral' | 'ok' }>> =
  {
    untested: { label: 'Non vérifiée', tone: 'neutral' },
    ok: { label: 'Déclaration vérifiée', tone: 'ok' },
  };

/**
 * One connector: its state, a test, and what may change here. A REST API's
 * test checks its declaration without calling it (ADR-112, ADR-113): it
 * says so, rather than a connection it did not make.
 */
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
  // Counts the connection tests: the tool policies start again from each.
  const [tests, setTests] = useState(0);
  const action = useAction();
  const type = data.types.find((t) => t.type === connector.type);
  const rest = connector.rest;
  // Also when its declaration can no longer be read: it is deleted here.
  const isRest = connector.type === 'rest';
  const status =
    (isRest && REST_STATUS[connector.status]) || (STATUS[connector.status] ?? STATUS.error);
  // Added from the administration: deleted here, and changed while readable.
  const added = Boolean(connector.mcp) || isRest;
  const editable = Boolean(connector.mcp ?? rest);

  async function test() {
    const result = await action.run(() =>
      api.post<ConnectorTest>(`/admin/connectors/${connector.id}/test`),
    );
    // Counted with its answer, in the same render: the tool policies start
    // again from this test's tools, not from the former ones.
    setTests((n) => n + 1);
    setTested(result);
    onChanged();
  }

  async function toggle() {
    const update = { enabled: !connector.enabled } satisfies ConnectorUpdate;
    if (await action.run(() => api.patch(`/admin/connectors/${connector.id}`, update))) {
      onChanged();
    }
  }

  // The rules of a former test are no rules for what was changed since: a
  // policy dropped by the change would be written back (ADR-113).
  function edit() {
    setTested(null);
    setEditing(true);
  }

  function done(changed: boolean) {
    setEditing(false);
    if (changed) {
      setTested(null);
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
      {rest && (
        <p className="text-sm text-muted">
          <code>{rest.url}</code> · outils <code>rest_{rest.prefix}_…</code> ·{' '}
          {rest.operations.length} opération(s) ·{' '}
          {rest.tokenStored
            ? `jeton envoyé en « ${rest.tokenHeader ?? 'Authorization: Bearer'} »`
            : 'sans jeton'}
        </p>
      )}
      {connector.lastError && connector.status === 'error' && !tested && (
        <ErrorNote message={`Dernier démarrage : ${connector.lastError}`} />
      )}
      <p className="text-xs text-muted">Modifié le {formatDate(connector.updatedAt)}</p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" loading={action.pending} onClick={test}>
          {isRest ? 'Vérifier la déclaration' : 'Tester la connexion'}
        </Button>
        {editable && !editing && (
          <Button variant="secondary" onClick={edit}>
            Modifier
          </Button>
        )}
        <Button variant="secondary" onClick={toggle}>
          {connector.enabled ? 'Désactiver' : 'Activer'}
        </Button>
        {added && (
          <Button variant="danger" onClick={remove}>
            Supprimer
          </Button>
        )}
      </div>
      {editing &&
        (rest ? (
          <RestForm data={data} connector={connector} onDone={done} />
        ) : (
          <McpForm data={data} connector={connector} onDone={done} />
        ))}
      {tested && !tested.ok && (
        <ErrorNote message={`Échec : ${tested.error ?? 'erreur inconnue'}`} />
      )}
      {tested?.ok && (
        <section className="flex flex-col gap-2">
          <p className="text-sm text-ok">
            {isRest
              ? `Déclaration vérifiée : ${tested.tools.length} outil(s). L’API n’est pas appelée ; le premier appel d’un run dira si elle répond.`
              : `Connexion réussie : ${tested.tools.length} outil(s) exposé(s).`}
          </p>
          {added && tested.tools.length > 0 && (
            <ToolPolicies
              // Each test brings the server's tools and their current rules: the
              // form starts again from them, never from a former test.
              key={tests}
              connectorId={connector.id}
              data={data}
              tools={tested.tools}
              // A REST operation's description is changed in its declaration.
              replaceableDescriptions={Boolean(connector.mcp)}
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
