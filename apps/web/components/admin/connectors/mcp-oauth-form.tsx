'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import { api, useAction, type ConnectorView, type McpOAuthDeclaration } from '@/lib/api';

/**
 * Declares OAuth on an MCP server (ADR-171): the client registered at its
 * authorization server, which the platform finds from the server itself;
 * its issuer only when the server names several, or none. Declared again,
 * everything is replaced, the secret too, and the authorization given
 * under the former declaration goes. A typed token goes as well: OAuth
 * replaces it.
 */
export function McpOAuthForm({
  connector,
  redirectUri,
  secretsAvailable,
  onDone,
}: {
  connector: ConnectorView;
  /** Where the authorization server sends the browser back. */
  redirectUri: string;
  secretsAvailable: boolean;
  onDone: (changed: boolean) => void;
}) {
  const declared = connector.mcp?.oauth ?? null;
  const [issuer, setIssuer] = useState(declared?.issuer ?? '');
  const [clientId, setClientId] = useState(declared?.clientId ?? '');
  const [secret, setSecret] = useState('');
  const [scopes, setScopes] = useState(declared?.scopes.join(' ') ?? '');
  const action = useAction();

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (
      declared?.authorization &&
      !window.confirm(
        'Déclarer de nouveau retire l’autorisation donnée : il faudra autoriser ce serveur de nouveau. Continuer ?',
      )
    ) {
      return;
    }
    const asked = scopes.trim();
    const body = {
      issuer: issuer.trim() || undefined,
      clientId,
      clientSecret: secret.trim() || undefined,
      scopes: asked ? asked.split(/\s+/) : undefined,
    } satisfies McpOAuthDeclaration;
    const saved = await action.run(() => api.put(`/admin/connectors/${connector.id}/oauth`, body));
    if (saved) onDone(true);
  }

  return (
    <form onSubmit={submit} className="flex flex-col gap-3">
      <p className="text-sm text-muted">
        Enregistrez d’abord la plateforme comme client auprès du serveur d’autorisation, avec
        l’adresse de retour <code>{redirectUri}</code>. La plateforme trouve ce serveur à partir du
        serveur MCP : n’indiquez son émetteur que s’il en annonce plusieurs, ou aucun.
      </p>
      <div className="grid gap-3 sm:grid-cols-2">
        <Field label="Identifiant du client">
          <Input
            required
            maxLength={255}
            autoComplete="off"
            value={clientId}
            onChange={(e) => setClientId(e.target.value)}
          />
        </Field>
        {secretsAvailable && (
          <Field
            label={
              declared
                ? 'Secret du client (à saisir de nouveau ; vide : client public)'
                : 'Secret du client (vide : client public)'
            }
          >
            <Input
              type="password"
              autoComplete="off"
              maxLength={512}
              value={secret}
              onChange={(e) => setSecret(e.target.value)}
            />
          </Field>
        )}
        <Field label="Émetteur du serveur d’autorisation (facultatif)">
          <Input
            type="url"
            maxLength={2000}
            placeholder="https://auth.exemple.fr"
            value={issuer}
            onChange={(e) => setIssuer(e.target.value)}
          />
        </Field>
        <Field label="Portées demandées, séparées par des espaces (vide : celles du serveur)">
          <Input maxLength={4100} value={scopes} onChange={(e) => setScopes(e.target.value)} />
        </Field>
      </div>
      {connector.mcp?.tokenStored && (
        <p className="text-xs text-muted">Le jeton enregistré est retiré : OAuth le remplace.</p>
      )}
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          Déclarer
        </Button>
        <Button
          type="button"
          variant="ghost"
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
