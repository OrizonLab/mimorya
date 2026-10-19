'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction } from '@/lib/api';
import type { ConnectorView, ConnectorsView } from './types';

/** Adds an MCP server, or changes its name, address and token. */
export function McpForm({
  data,
  connector,
  onDone,
}: {
  data: ConnectorsView;
  connector?: ConnectorView;
  onDone: (changed: boolean) => void;
}) {
  const [name, setName] = useState(connector?.name ?? '');
  const [prefix, setPrefix] = useState(connector?.mcp?.prefix ?? '');
  const [url, setUrl] = useState(connector?.mcp?.url ?? '');
  const [token, setToken] = useState('');
  const action = useAction();
  const moved = connector?.mcp && url.trim() !== connector.mcp.url;

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() =>
      connector
        ? api.patch(`/admin/connectors/${connector.id}`, {
            name,
            url,
            ...(token.trim() ? { token: token.trim() } : {}),
          })
        : api.post('/admin/connectors', {
            name,
            prefix,
            url,
            ...(token.trim() ? { token: token.trim() } : {}),
          }),
    );
    if (saved) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Nom">
          <Input
            required
            minLength={2}
            maxLength={80}
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </Field>
        <Field label="Préfixe des outils (définitif)">
          <Input
            required
            disabled={Boolean(connector)}
            pattern="[a-z][a-z0-9]{1,15}"
            placeholder="docs"
            value={prefix}
            onChange={(e) => setPrefix(e.target.value)}
          />
        </Field>
        <Field
          label={data.allowPrivateNetwork ? 'Adresse du serveur' : 'Adresse du serveur (https)'}
        >
          <Input
            required
            type="url"
            maxLength={2000}
            placeholder="https://mcp.exemple.fr/mcp"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
          />
        </Field>
        {data.secretsAvailable && (
          <Field
            label={
              connector?.mcp?.tokenStored && !moved
                ? 'Jeton d’accès (laisser vide pour conserver le jeton enregistré)'
                : 'Jeton d’accès (facultatif)'
            }
          >
            <Input
              type="password"
              autoComplete="off"
              maxLength={2000}
              value={token}
              onChange={(e) => setToken(e.target.value)}
            />
          </Field>
        )}
      </div>
      <p className="text-xs text-muted">
        Le jeton (lettres sans accent, chiffres et - . _ ~ + /, puis éventuellement des =) est
        envoyé en en-tête « Authorization: Bearer », chiffré sur le serveur et jamais réaffiché. Il
        ne suit pas le connecteur vers une autre adresse.{' '}
        {data.allowPrivateNetwork
          ? 'Ce déploiement autorise les serveurs de son réseau privé.'
          : 'Seules les adresses publiques en https sont acceptées.'}
      </p>
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {connector ? 'Enregistrer' : 'Ajouter le serveur'}
        </Button>
        <Button type="button" variant="ghost" onClick={() => onDone(false)}>
          Annuler
        </Button>
      </div>
    </form>
  );
}
