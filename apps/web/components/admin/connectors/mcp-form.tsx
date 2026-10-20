'use client';

import { useState, type FormEvent } from 'react';
import { Button, ErrorNote, Field, Input } from '@/components/ui';
import {
  ApiError,
  api,
  useAction,
  type ConnectorTest,
  type ConnectorTrial,
  type ConnectorUpdate,
  type ConnectorView,
  type Connectors,
  type NewConnector,
} from '@/lib/api';

/** What a trial answered, for the values it tried. */
interface Trial {
  typed: string;
  /** The server was reached, or not (`ok`). */
  result?: ConnectorTest;
  /** The values were refused before any connection. */
  refused?: string;
}

/**
 * Adds an MCP server, or changes its name, address and token. The address
 * and token typed can be tried before they are saved (ADR-050).
 */
export function McpForm({
  data,
  connector,
  onDone,
}: {
  data: Connectors;
  connector?: ConnectorView;
  onDone: (changed: boolean) => void;
}) {
  const [name, setName] = useState(connector?.name ?? '');
  const [prefix, setPrefix] = useState(connector?.mcp?.prefix ?? '');
  const [url, setUrl] = useState(connector?.mcp?.url ?? '');
  const [token, setToken] = useState('');
  const action = useAction();
  const [trying, setTrying] = useState(false);
  const moved = connector?.mcp && url.trim() !== connector.mcp.url;
  // A saved server keeps its token, which a trial never sends: its own test,
  // on its card, tries it with that token.
  const triable = !connector || moved || token.trim() !== '';
  // A trial answers for what was typed then, whether the server was reached
  // or the values refused: shown only while they still are.
  const typed = JSON.stringify([prefix, url, token]);
  const [tried, setTried] = useState<Trial | null>(null);
  const trial = tried?.typed === typed ? tried : null;

  async function tryServer() {
    const sent = typed;
    setTrying(true);
    try {
      const result = await api.post<ConnectorTest>('/admin/connectors/trial', {
        prefix,
        url,
        token: token.trim() || undefined,
      } satisfies ConnectorTrial);
      setTried({ typed: sent, result });
    } catch (e: unknown) {
      const refused = e instanceof ApiError ? e.message : 'Essai impossible, réessayez.';
      setTried({ typed: sent, refused });
    } finally {
      setTrying(false);
    }
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    const saved = await action.run(() =>
      connector
        ? api.patch(`/admin/connectors/${connector.id}`, {
            name,
            url,
            // Left out when blank: the stored token is kept.
            token: token.trim() || undefined,
          } satisfies ConnectorUpdate)
        : api.post('/admin/connectors', {
            name,
            prefix,
            url,
            token: token.trim() || undefined,
          } satisfies NewConnector),
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
      {trial?.refused && <ErrorNote message={trial.refused} />}
      {trial?.result && !trial.result.ok && (
        <ErrorNote message={`Échec : ${trial.result.error ?? 'erreur inconnue'}`} />
      )}
      {trial?.result?.ok && (
        <p className="text-sm text-ok">
          Connexion réussie : {trial.result.tools.length} outil(s) exposé(s)
          {trial.result.tools.length > 0 &&
            ` (${trial.result.tools.map((t) => t.name).join(', ')})`}
          . Rien n’est enregistré tant que le serveur n’est pas ajouté.
        </p>
      )}
      {action.error && <ErrorNote message={action.error} />}
      <div className="flex gap-2">
        <Button type="submit" loading={action.pending}>
          {connector ? 'Enregistrer' : 'Ajouter le serveur'}
        </Button>
        {triable && (
          <Button type="button" variant="secondary" loading={trying} onClick={tryServer}>
            Tester avant d’enregistrer
          </Button>
        )}
        <Button
          type="button"
          variant="ghost"
          // Closed before its change is answered, it could be reopened from the
          // former resource, and that change overwritten.
          disabled={action.pending}
          onClick={() => onDone(false)}
        >
          Annuler
        </Button>
      </div>
    </form>
  );
}
