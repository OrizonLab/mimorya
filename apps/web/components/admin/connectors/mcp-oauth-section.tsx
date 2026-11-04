'use client';

import { useState } from 'react';
import { Badge, Button, ErrorNote } from '@/components/ui';
import { api, useAction, type ConnectorView, type OAuthAuthorizationStart } from '@/lib/api';
import { formatDate } from '@/lib/format';
import { McpOAuthForm } from './mcp-oauth-form';

/**
 * An MCP server's OAuth on its card (ADR-171): declared or not, what was
 * found (its authorization server, the client, the scopes), its
 * authorization and its renewal; authorized at the authorization server,
 * where the browser goes and comes back to the administration, and
 * withdrawn. A declaration that no longer reads is withdrawn first.
 */
export function McpOAuthSection({
  connector,
  redirectUri,
  secretsAvailable,
  onChanged,
}: {
  connector: ConnectorView;
  redirectUri: string;
  secretsAvailable: boolean;
  onChanged: () => void;
}) {
  const [declaring, setDeclaring] = useState(false);
  const action = useAction();
  const oauth = connector.mcp?.oauth ?? null;
  const authorization = oauth?.authorization ?? null;
  const base = `/admin/connectors/${connector.id}/oauth`;

  async function remove(path: string, question: string) {
    if (!window.confirm(question)) return;
    if (await action.run(() => api.delete(path))) onChanged();
  }

  // The browser leaves for the authorization server, the attempt sealed in
  // its cookie; it comes back to the administration.
  async function authorize() {
    const started = await action.run(() =>
      api.post<OAuthAuthorizationStart>(`${base}/authorization`, {}),
    );
    if (started) window.location.assign(started.redirectUrl);
  }

  if (connector.oauthUnreadable) {
    return (
      <section className="flex flex-col gap-2">
        {/* A state of the server, like its status: not an alert. */}
        <p className="flex flex-wrap items-center gap-2 text-sm">
          <Badge tone="danger">OAuth illisible</Badge>
          <span>Déclaration OAuth illisible : retirez-la, puis déclarez-la de nouveau.</span>
        </p>
        {action.error && <ErrorNote message={action.error} />}
        <div>
          <Button
            variant="secondary"
            loading={action.pending}
            onClick={() => remove(base, 'Retirer la déclaration OAuth illisible de ce serveur ?')}
          >
            Retirer la déclaration OAuth
          </Button>
        </div>
      </section>
    );
  }

  const form = declaring && (
    <McpOAuthForm
      connector={connector}
      redirectUri={redirectUri}
      secretsAvailable={secretsAvailable}
      onDone={(changed) => {
        setDeclaring(false);
        if (changed) onChanged();
      }}
    />
  );

  if (!oauth) {
    return declaring ? (
      <section className="flex flex-col gap-2 rounded-md border border-line p-3">
        <p className="text-sm font-medium">Autorisation OAuth</p>
        {form}
      </section>
    ) : (
      <div>
        <Button variant="ghost" onClick={() => setDeclaring(true)}>
          Déclarer OAuth
        </Button>
      </div>
    );
  }

  const client =
    oauth.clientAuth === 'none'
      ? 'client public'
      : oauth.clientSecretStored
        ? 'secret enregistré'
        : 'secret absent';
  return (
    <section className="flex flex-col gap-2 rounded-md border border-line p-3 text-sm">
      <p className="font-medium">Autorisation OAuth</p>
      <p className="text-muted">
        Serveur d’autorisation <code>{oauth.issuer}</code> · client <code>{oauth.clientId}</code> (
        {client}) · portées{' '}
        {oauth.scopes.length > 0 ? <code>{oauth.scopes.join(' ')}</code> : 'aucune'}
      </p>
      {authorization ? (
        <p className="flex flex-wrap items-center gap-2">
          <Badge tone="ok">Autorisé</Badge>
          <span>
            Autorisé le {formatDate(authorization.authorizedAt)}
            {authorization.expiresAt
              ? ` · jeton valable jusqu’au ${formatDate(authorization.expiresAt)}`
              : ' · jeton sans échéance'}
            {authorization.renewable && ' · renouvelé avant son échéance'}
            {authorization.renewedAt &&
              ` · dernier renouvellement le ${formatDate(authorization.renewedAt)}`}
          </span>
        </p>
      ) : (
        <p className="flex flex-wrap items-center gap-2">
          <Badge tone="warn">Non autorisé</Badge>
          <span className="text-muted">
            Les runs n’ont pas les outils de ce serveur tant qu’il n’est pas autorisé.
          </span>
        </p>
      )}
      {authorization?.renewalFailure && (
        <ErrorNote message={`Dernier renouvellement échoué : ${authorization.renewalFailure}`} />
      )}
      {action.error && <ErrorNote message={action.error} />}
      {!declaring && (
        <div className="flex flex-wrap gap-2">
          <Button variant="secondary" loading={action.pending} onClick={authorize}>
            {authorization ? 'Autoriser de nouveau' : 'Autoriser'}
          </Button>
          {authorization && (
            <Button
              variant="secondary"
              onClick={() =>
                remove(
                  `${base}/authorization`,
                  'Retirer l’autorisation ? Les runs n’auront plus les outils de ce serveur ; le serveur d’autorisation garde son accord tant qu’il n’est pas révoqué chez lui.',
                )
              }
            >
              Retirer l’autorisation
            </Button>
          )}
          <Button variant="ghost" onClick={() => setDeclaring(true)}>
            Modifier la déclaration
          </Button>
          <Button
            variant="danger"
            onClick={() =>
              remove(
                base,
                'Retirer OAuth de ce serveur, avec le secret du client et l’autorisation ?',
              )
            }
          >
            Retirer OAuth
          </Button>
        </div>
      )}
      {form}
    </section>
  );
}
